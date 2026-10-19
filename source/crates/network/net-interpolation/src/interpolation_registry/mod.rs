use crate::archetype_set::ArchetypeSet;
use crate::frame_source::FrameSource;
use crate::history_component::HistoryComponent;
use crate::interpolation_holders::Holders;
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::member_mode::MemberMode;
use crate::registered_rule::RegisteredRule;
use crate::rule_resolution::{ResolvedRule, RuleResolutionScratch, RuleTarget};
use bevy_app::App;

use bevy_ecs::archetype::{Archetype, Archetypes};
use bevy_ecs::component::{ComponentId, Components};
use bevy_ecs::prelude::*;
use bevy_ecs::query::ComponentIdSet;
use bevy_platform::collections::{HashMap, HashSet};
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::ReplicationKind;

/// Stores interpolation functions and rule selection metadata.
///
/// The registry is managed by `InterpolationPlugin` and the
/// registration APIs. Most users should not mutate it directly; use
/// `InterpolationRegistrationExt::interpolate`
/// or `AppInterpolationExt`.
#[derive(Resource, Debug, Default)]
pub struct InterpolationRegistry {
    /// All registered interpolation rules in insertion order.
    ///
    /// [`InterpolationRuleId`] is an index into this vector. Equal-priority
    /// rules preserve this order, matching Replicon's "first registered wins"
    /// behavior for ties.
    rules: Vec<RegisteredRule>,
    /// The frame history components of every frame rule, each once.
    frame_history_component_ids: Vec<ComponentId>,
    /// Where frame interpolation reads each component's samples in place of its frame history,
    /// as another plugin set it.
    frame_sources: HashMap<ComponentTypeId, FrameSource>,
    /// Whether plugin finalization has run, after which a rule would install no system.
    finalized: bool,
    /// The components whose confirmed history system is installed, each once, and how each
    /// replicates.
    history_systems: HashMap<ComponentTypeId, ReplicationKind>,
    /// The components whose frame history systems are installed, each once.
    frame_history_systems: HashSet<ComponentTypeId>,
    /// The rules registered so far, built by [`finish`] once every member's mode is known.
    pending: PendingRules,
    /// How each component's confirmed history is written, as its registration recorded it.
    member_modes: HashMap<ComponentTypeId, MemberMode>,
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
    registry.assert_not_finalized();
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
    /// Records how `C`'s confirmed history is written.
    ///
    /// # Panics
    ///
    /// When `C` recorded the other mode: one component interpolates one way.
    /// The mode `type_id` recorded, if it did.
    pub(crate) fn member_mode(&self, type_id: &ComponentTypeId) -> Option<MemberMode> {
        self.member_modes.get(type_id).copied()
    }

    /// Queues `rule` for [`finish`].
    pub(crate) fn defer(&mut self, rule: PendingRule) {
        self.pending.0.push(rule);
    }

    pub(crate) fn record_member<C: HistoryComponent>(&mut self, mode: MemberMode) {
        let previous = self.member_modes.insert(ComponentTypeId::of::<C>(), mode);
        assert!(
            previous.is_none_or(|previous| previous.replication() == mode.replication()),
            "{} registers for interpolation in two modes",
            core::any::type_name::<C>()
        );
    }

    const FINALIZED_RULE_REGISTRATION_ERROR: &'static str =
        "cannot register interpolation rules after InterpolationRegistry has been finalized";

    pub(crate) fn finalize(&mut self) {
        if self.finalized {
            return;
        }
        self.frame_history_component_ids = self
            .rules
            .iter()
            .flat_map(|rule| &rule.members)
            .filter(|member| member.frame.is_some())
            .map(|member| member.frame_history_component_id)
            .collect::<ComponentIdSet>()
            .into_iter()
            .collect();
        self.finalized = true;
    }

    /// Has frame interpolation read `C`'s samples through `source` on the archetypes that hold
    /// its component, in place of a frame history of `C`.
    ///
    /// # Panics
    ///
    /// After the app finished its plugins, which finalizes the registry.
    pub fn set_frame_source<C: 'static>(&mut self, source: FrameSource) {
        self.assert_not_finalized();
        self.frame_sources
            .insert(ComponentTypeId::of::<C>(), source);
    }

    /// Where frame interpolation reads `type_id`'s samples in place of its frame history.
    pub(crate) fn frame_source(&self, type_id: &ComponentTypeId) -> Option<&FrameSource> {
        self.frame_sources.get(type_id)
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
        if let Some(&installed) = self.history_systems.get(&type_id) {
            assert_eq!(
                installed, replication,
                "a component's confirmed history is maintained in one mode"
            );
            return false;
        }
        self.history_systems.insert(type_id, replication);
        true
    }

    /// Whether `type_id`'s frame history systems are still to install, which they then are.
    pub(crate) fn install_frame_history_once(&mut self, type_id: ComponentTypeId) -> bool {
        self.frame_history_systems.insert(type_id)
    }

    fn assert_not_finalized(&self) {
        assert!(
            !self.finalized,
            "{}",
            Self::FINALIZED_RULE_REGISTRATION_ERROR
        );
    }

    /// A rule by ID.
    ///
    /// # Panics
    ///
    /// Panics if `rule_id` was not produced by this registry.
    /// Resolves into `holders` the archetypes created since their last refresh.
    pub(crate) fn refresh_holders(
        &self,
        holders: &mut Holders,
        archetypes: &Archetypes,
        components: &Components,
    ) {
        let old_generation = core::mem::replace(&mut holders.generation, archetypes.generation());
        let Holders {
            target,
            marker_component_id,
            apply,
            history,
            sourced,
            scratch,
            ..
        } = holders;
        for archetype in archetypes[old_generation..]
            .iter()
            .filter(|archetype| archetype.contains(*marker_component_id))
        {
            let id = archetype.id();
            for resolved in
                self.resolved_rules_for_archetype(components, archetype, *target, scratch)
            {
                let rule = self.rule(resolved.rule_id);
                if resolved.holds_apply {
                    let rule_index = resolved.rule_id.0;
                    if apply.len() <= rule_index {
                        apply.resize(rule_index + 1, ArchetypeSet::new());
                    }
                    apply[rule_index].insert(id);
                }
                for member in &rule.members {
                    let maintains = match target {
                        RuleTarget::Delayed => member.confirmed.is_some(),
                        RuleTarget::Frame => member.frame.is_some(),
                    };
                    if resolved.holds_history && maintains {
                        history.entry(member.type_id).or_default().insert(id);
                    }
                    if member
                        .frame_source
                        .as_ref()
                        .is_some_and(|source| archetype.contains(source.component_id))
                    {
                        sourced.entry(member.type_id).or_default().insert(id);
                    }
                }
            }
        }
    }

    pub(crate) fn rule(&self, rule_id: InterpolationRuleId) -> &RegisteredRule {
        self.rules
            .get(rule_id.0)
            .expect("interpolation rule ID should belong to this registry")
    }

    /// The `FrameInterpolationHistory` component IDs held by frame rules, set when the registry
    /// is finalized.
    ///
    /// Removing these from an entity re-seeds its frame interpolation: the
    /// history update re-creates them from the live value on its next run
    /// (see `frame_update_inserts_history_when_frame_interpolate_is_added`).
    /// Timeline switches use this when re-adding `FrameInterpolate` after an
    /// era without the marker, during which the histories froze at a stale
    /// value that restore would otherwise pin back onto live simulation.
    pub fn frame_history_component_ids(&self) -> &[ComponentId] {
        &self.frame_history_component_ids
    }

    /// Resolves history and apply holding for one archetype.
    ///
    /// Candidates are ordered by descending priority and then by registration
    /// order. History and apply are then resolved independently, with separate
    /// claimed-member sets:
    ///
    /// - For history, a member is available when either its live component or
    ///   its history selected by `target` is present.
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
        target: RuleTarget,
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
                    || match target {
                        RuleTarget::Delayed => {
                            archetype.contains(member.confirmed_history_component_id)
                        }
                        RuleTarget::Frame => archetype.contains(member.frame_history_component_id),
                    }
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
        self.assert_not_finalized();
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
