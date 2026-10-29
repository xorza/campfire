use crate::zero_hour::additions::Additions;
use crate::zero_hour::import_names::ProductionNames;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// What a production's conversion reads and gives as it runs: the install's rules, the names of
/// the unit types it makes, and the additions it gives them.
#[derive(Debug)]
pub(crate) struct ProductionCx<'r, 'a> {
    pub(crate) rules: &'r ZeroHourRules,
    pub(crate) names: &'a mut ProductionNames<'r>,
    pub(crate) additions: &'a mut Additions,
}
