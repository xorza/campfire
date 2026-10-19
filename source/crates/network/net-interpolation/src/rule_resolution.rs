//! What the registry resolves an archetype into, for each pipeline's owners.

use crate::interpolation_rule_id::InterpolationRuleId;
use campfire_net_replication::ComponentKind;

/// History representation used when resolving history ownership.
///
/// This affects only the history lane. Apply ownership always requires every
/// live component targeted by the rule.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RuleTarget {
    /// Delayed interpolation uses live components or confirmed histories.
    Delayed,
    /// Frame interpolation and correction use live components or frame histories.
    Frame,
}

/// The work assigned to one interpolation rule for an archetype.
///
/// The resolver selects rules independently for two jobs:
///
/// - `owns_history` selects which rule's member callbacks create and update
///   histories and insert or remove delayed live components.
/// - `owns_apply` selects which rule's interpolation callback writes the live
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
    pub owns_history: bool,
    /// Whether this rule wins the interpolation-application lane for all of its members.
    pub owns_apply: bool,
}

/// Reusable temporary storage for `InterpolationRegistry::resolved_rules_for_archetype`.
///
/// Each pipeline's owners keep one and reuse its vector capacities while resolving
/// newly-created archetypes.
#[derive(Debug, Default)]
pub(crate) struct RuleResolutionScratch {
    pub(crate) candidates: Vec<InterpolationRuleId>,
    pub(crate) history_claimed_members: Vec<ComponentKind>,
    pub(crate) apply_claimed_members: Vec<ComponentKind>,
    pub(crate) resolved_rules: Vec<ResolvedRule>,
}
