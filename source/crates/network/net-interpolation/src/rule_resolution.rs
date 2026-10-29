//! What the registry resolves an archetype into, for the holders.
use campfire_net_replication::ComponentTypeId;

use crate::interpolation_rule_id::InterpolationRuleId;

/// The work assigned to one interpolation rule for an archetype.
///
/// The resolver selects rules independently for two jobs:
///
/// - `holds_history` selects which rule's member callbacks create and update
///   histories and insert or remove delayed live components.
/// - `holds_apply` selects which rule's interpolation callback writes the live
///   components.
///
/// Winning one job does not affect resolution of the other. A rule without the
/// corresponding callbacks still reserves its members for that job, so a
/// lower-priority rule cannot run instead.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct ResolvedRule {
    /// Rule receiving work in at least one lane.
    pub rule_id: InterpolationRuleId,
    /// Whether this rule wins the history-policy lane for all of its members.
    pub holds_history: bool,
    /// Whether this rule wins the interpolation-application lane for all of its members.
    pub holds_apply: bool,
}

/// Reusable temporary storage for `InterpolationRegistry::resolved_rules_for_archetype`.
///
/// The holders keep one and reuse its vector capacities while resolving newly-created
/// archetypes.
#[derive(Debug, Default)]
pub(crate) struct RuleResolutionScratch {
    pub(crate) candidates: Vec<InterpolationRuleId>,
    pub(crate) history_claimed_members: Vec<ComponentTypeId>,
    pub(crate) apply_claimed_members: Vec<ComponentTypeId>,
    pub(crate) resolved_rules: Vec<ResolvedRule>,
}
