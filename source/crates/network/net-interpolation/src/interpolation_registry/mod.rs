use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::member_mode::MemberMode;
use crate::registered_rule::RegisteredRule;
use crate::rule_resolution::{ResolvedRule, RuleResolutionScratch};
use bevy_app::App;
use campfire_net_replication::HistoryComponent;

use bevy_ecs::archetype::{Archetype, Archetypes};
use bevy_ecs::component::Components;
use bevy_ecs::prelude::*;
use campfire_net_core::RegistrySeal;
use campfire_net_frame_interpolation::ArchetypeSet;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::ReplicationKind;

/// The interpolation rules and what each component interpolates with, which the registration
/// APIs, [`InterpolationRegistrationExt::interpolate`](crate::InterpolationRegistrationExt) and
/// [`AppInterpolationExt`](crate::AppInterpolationExt), fill and `InterpolationRegistryPlugin`
/// seals as the app finishes its plugins.
#[derive(Resource, Debug, Default)]
pub struct InterpolationRegistry {
    /// All registered interpolation rules in insertion order.
    ///
    /// [`InterpolationRuleId`] is an index into this vector. Equal-priority
    /// rules preserve this order, matching Replicon's "first registered wins"
    /// behavior for ties.
    rules: Vec<RegisteredRule>,
    /// Sealed as the app finishes its plugins, after which a rule would install no system.
    seal: RegistrySeal<()>,
    /// The rules registered so far, built by [`finish`] once every member's mode is known.
    pending: PendingRules,
    /// What the registry holds of each component, sorted by its type.
    components: Vec<ComponentEntry>,
}

/// What the registry holds of one component.
#[derive(Debug)]
struct ComponentEntry {
    type_id: ComponentTypeId,
    /// How its confirmed history is written, as its registration recorded it.
    mode: Option<MemberMode>,
    /// How its installed confirmed history system takes its replication.
    history_system: Option<ReplicationKind>,
    /// Whether its receive fns are set under the interpolation markers.
    marker_fns: bool,
}

/// A registered rule, built once the app finishes its plugins.
pub(crate) type PendingRule = Box<dyn FnOnce(&mut App) + Send + Sync>;

#[derive(Default)]
struct PendingRules(Vec<PendingRule>);

impl core::fmt::Debug for PendingRules {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PendingRules")
            .field("len", &self.0.len())
            .finish()
    }
}

/// The app's interpolation registry, created by its first registration, which must come before
/// the app finishes its plugins.
pub(crate) fn open(app: &mut App) -> Mut<'_, InterpolationRegistry> {
    let registry = app
        .world_mut()
        .get_resource_or_init::<InterpolationRegistry>();
    registry.assert_open();
    registry
}

/// Builds every registered rule, now that each member's mode is known, and finalizes the
/// registry: `InterpolationRegistryPlugin::finish`.
pub(crate) fn finish(app: &mut App) {
    let pending = core::mem::take(&mut open(app).pending.0);
    for rule in pending {
        rule(app);
    }
    app.world_mut()
        .resource_mut::<InterpolationRegistry>()
        .finalize();
}

impl InterpolationRegistry {
    /// The mode `type_id` recorded, if it did.
    pub(crate) fn member_mode(&self, type_id: &ComponentTypeId) -> Option<MemberMode> {
        self.component(type_id)?.mode
    }

    /// Queues `rule` for [`finish`].
    pub(crate) fn defer(&mut self, rule: PendingRule) {
        self.pending.0.push(rule);
    }

    /// Records how `C`'s confirmed history is written.
    ///
    /// # Panics
    ///
    /// When `C` recorded the other mode: one component interpolates one way.
    pub(crate) fn record_member<C: HistoryComponent>(&mut self, mode: MemberMode) {
        let previous = self
            .component_mut(ComponentTypeId::of::<C>())
            .mode
            .replace(mode);
        assert!(
            previous.is_none_or(|previous| previous.replication() == mode.replication()),
            "{} registers for interpolation in two modes",
            core::any::type_name::<C>()
        );
    }

    const NAME: &'static str = "interpolation registry";

    pub(crate) fn finalize(&mut self) {
        self.seal.seal(Self::NAME, ());
    }

    /// Whether `type_id`'s confirmed history system, for `replication`, is still to install, which
    /// it then is.
    ///
    /// # Panics
    ///
    /// When `type_id`'s system is installed for the other replication: one component's confirmed
    /// history is written one way, by one system, whichever rule holds it.
    pub(crate) fn install_history_once(
        &mut self,
        type_id: ComponentTypeId,
        replication: ReplicationKind,
    ) -> bool {
        let installed = self
            .component_mut(type_id)
            .history_system
            .replace(replication);
        assert!(
            installed.is_none_or(|installed| installed == replication),
            "a component's confirmed history is maintained in one mode"
        );
        installed.is_none()
    }

    /// Whether `type_id`'s receive fns are still to set under the interpolation markers, which
    /// they then are.
    pub(crate) fn set_marker_fns_once(&mut self, type_id: ComponentTypeId) -> bool {
        !core::mem::replace(&mut self.component_mut(type_id).marker_fns, true)
    }

    fn component(&self, type_id: &ComponentTypeId) -> Option<&ComponentEntry> {
        self.components
            .binary_search_by_key(type_id, |entry| entry.type_id)
            .ok()
            .map(|index| &self.components[index])
    }

    /// The entry of `type_id`, which goes in if it is not there.
    fn component_mut(&mut self, type_id: ComponentTypeId) -> &mut ComponentEntry {
        let index = self
            .components
            .binary_search_by_key(&type_id, |entry| entry.type_id)
            .unwrap_or_else(|index| {
                self.components.insert(
                    index,
                    ComponentEntry {
                        type_id,
                        mode: None,
                        history_system: None,
                        marker_fns: false,
                    },
                );
                index
            });
        &mut self.components[index]
    }

    fn assert_open(&self) {
        self.seal.assert_open(Self::NAME);
    }

    /// Resolves into `holders` the archetypes created since their last refresh.
    pub(crate) fn refresh_holders(
        &self,
        holders: &mut InterpolationHolders,
        archetypes: &Archetypes,
        components: &Components,
    ) {
        let old_generation = core::mem::replace(&mut holders.generation, archetypes.generation());
        let InterpolationHolders {
            marker_component_id,
            apply,
            history,
            scratch,
            ..
        } = holders;
        for archetype in archetypes[old_generation..]
            .iter()
            .filter(|archetype| archetype.contains(*marker_component_id))
        {
            let id = archetype.id();
            for resolved in self.resolved_rules_for_archetype(components, archetype, scratch) {
                let rule = self.rule(resolved.rule_id);
                if resolved.holds_apply {
                    let rule_index = resolved.rule_id.0;
                    if apply.len() <= rule_index {
                        apply.resize(rule_index + 1, ArchetypeSet::new());
                    }
                    apply[rule_index].insert(id);
                }
                for member in &rule.members {
                    if resolved.holds_history && member.confirmed.is_some() {
                        history.entry(member.type_id).or_default().insert(id);
                    }
                }
            }
        }
    }

    /// A rule by ID.
    ///
    /// # Panics
    ///
    /// When this registry did not make `rule_id`.
    pub(crate) fn rule(&self, rule_id: InterpolationRuleId) -> &RegisteredRule {
        self.rules
            .get(rule_id.0)
            .expect("interpolation rule ID should belong to this registry")
    }

    /// Resolves history and apply holding for one archetype.
    ///
    /// Candidates are ordered by descending priority and then by registration
    /// order. History and apply are then resolved independently, with separate
    /// claimed-member sets:
    ///
    /// - For history, a member is available when either its live component or
    ///   its confirmed history is present.
    /// - For apply, a member is available only when its live component is
    ///   present. History presence does not affect apply selection.
    /// - A winning rule claims all of its members for that job. A later rule
    ///   cannot reuse any of them for the same job.
    ///
    /// The jobs must be independent because history maintenance controls
    /// delayed component presence. If replication has received `B` into
    /// `History<B>` but the interpolation timeline has not reached B's
    /// insertion tick, the entity has `(A, History<A>, History<B>)`. The bundle
    /// cannot apply without live `B`, but it must still win history so its
    /// history callbacks continue running and eventually insert live `B`.
    /// Meanwhile, the standalone `A` rule can keep interpolating live `A`.
    ///
    /// For example, consider a higher-priority `(A, B)` rule and a
    /// lower-priority `A` rule, both registered with
    /// `InterpolationFns::with`:
    ///
    /// - With `(A, B, History<A>)`, both live members exist. The bundle wins
    ///   both jobs even though `History<B>` is missing. Missing history does not
    ///   make the resolver fall back to `A`; the bundle's history callback will
    ///   normally create `History<B>`.
    /// - With `(A, History<A>, History<B>)`, live `B` is absent during delayed
    ///   insertion or after delayed removal. The bundle wins history because
    ///   both members have a history representation, but it cannot win apply.
    ///   The `A` rule therefore applies live `A` while the bundle maintains both
    ///   histories.
    /// - With `(A, History<A>)`, neither live `B` nor `History<B>` exists. The
    ///   bundle is unavailable for both jobs, so the `A` rule wins both.
    ///
    /// History preparation runs before interpolation application. If creating
    /// a missing history inserts or removes a live component, the apply phase
    /// resolves the resulting archetype before choosing its apply rule.
    pub(crate) fn resolved_rules_for_archetype<'a>(
        &self,
        components: &Components,
        archetype: &Archetype,
        scratch: &'a mut RuleResolutionScratch,
    ) -> &'a [ResolvedRule] {
        let RuleResolutionScratch {
            candidates,
            history_claimed_members,
            apply_claimed_members,
            resolved_rules,
        } = scratch;
        candidates.clear();
        history_claimed_members.clear();
        apply_claimed_members.clear();
        resolved_rules.clear();

        candidates.extend(
            self.rules
                .iter()
                .enumerate()
                .filter(|(_, rule)| (rule.matches_archetype)(components, archetype))
                .map(|(index, _)| InterpolationRuleId(index)),
        );
        candidates.sort_by(|lhs, rhs| {
            self.rules[rhs.0]
                .priority
                .cmp(&self.rules[lhs.0].priority)
                .then_with(|| lhs.0.cmp(&rhs.0))
        });
        for rule_id in candidates.iter().copied() {
            let rule = &self.rules[rule_id.0];
            let history = rule.members.iter().all(|member| {
                archetype.contains(member.live_component_id)
                    || archetype.contains(member.confirmed_history_component_id)
            }) && !rule
                .members()
                .any(|member| history_claimed_members.contains(&member));
            let apply = rule
                .members
                .iter()
                .all(|member| archetype.contains(member.live_component_id))
                && !rule
                    .members()
                    .any(|member| apply_claimed_members.contains(&member));
            if history {
                history_claimed_members.extend(rule.members());
            }
            if apply {
                apply_claimed_members.extend(rule.members());
            }
            if history || apply {
                resolved_rules.push(ResolvedRule {
                    rule_id,
                    holds_history: history,
                    holds_apply: apply,
                });
            }
        }
        resolved_rules
    }

    pub(crate) fn insert_rule(&mut self, rule: RegisteredRule) -> InterpolationRuleId {
        self.assert_open();
        for (index, member) in rule.members.iter().enumerate() {
            assert!(
                !rule.members[..index]
                    .iter()
                    .any(|previous| previous.type_id == member.type_id),
                "interpolation bundle rules cannot contain duplicate component types"
            );
        }

        let rule_id = InterpolationRuleId(self.rules.len());
        self.rules.push(rule);
        rule_id
    }

    /// Returns `true` if any interpolation rule covers component `C`.
    pub fn is_interpolated<C: Component>(&self) -> bool {
        let type_id = ComponentTypeId::of::<C>();
        self.rules
            .iter()
            .any(|rule| rule.members().any(|member| member == type_id))
    }
}

#[cfg(test)]
mod tests;
