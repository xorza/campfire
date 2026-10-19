use crate::interpolation_rule_component::InterpolationRuleComponent;
use bevy_ecs::archetype::Archetype;
use bevy_ecs::component::Components;
use bevy_ecs::query::ArchetypeFilter;
use campfire_net_replication::ComponentTypeId;

/// Returns whether a rule's filter matches an archetype.
pub(crate) type MatchesArchetypeFn = fn(&Components, &Archetype) -> bool;

/// One interpolation rule registered by `AppInterpolationExt`,
/// as the registry resolves it against each archetype: its typed systems were installed when it
/// was built.
///
/// A rule has a list of real component `members` it holds or writes, and an archetype filter.
/// Matching rules are sorted by priority before the registry resolves overlap between component
/// and bundle candidates.
#[derive(Debug)]
pub(crate) struct RegisteredRule {
    /// The components this rule holds, with the history it keeps of each.
    /// Bundle rules have more than one member.
    pub(crate) members: Vec<InterpolationRuleComponent>,
    /// Higher-priority rules are selected before lower-priority rules.
    pub(crate) priority: usize,
    /// Archetype-level filter predicate compiled from the rule filter type.
    pub(crate) matches_archetype: MatchesArchetypeFn,
}

impl RegisteredRule {
    pub(crate) fn new<F: ArchetypeFilter + 'static>(
        components: &Components,
        members: Vec<InterpolationRuleComponent>,
        priority: usize,
    ) -> Self {
        assert!(
            F::get_state(components).is_some(),
            "interpolation rule filters should be initialized before registration"
        );
        Self {
            members,
            priority,
            matches_archetype: matches_filter::<F>,
        }
    }

    pub(crate) fn members(&self) -> impl Iterator<Item = ComponentTypeId> + '_ {
        self.members.iter().map(|member| member.type_id)
    }
}

fn matches_filter<F>(components: &Components, archetype: &Archetype) -> bool
where
    F: ArchetypeFilter + 'static,
{
    F::get_state(components)
        .is_some_and(|state| F::matches_component_set(&state, &|id| archetype.contains(id)))
}
