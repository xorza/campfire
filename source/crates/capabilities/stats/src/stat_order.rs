use crate::stat_id::StatId;

/// The places of the mode's stats in the order the stats refresh computes them, which the mode's
/// stat graph gives: what the load gives the stat book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatOrder(pub Vec<StatId>);
