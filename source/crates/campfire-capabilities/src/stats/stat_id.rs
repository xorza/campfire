use serde::{Deserialize, Serialize};

use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::name_script_arg;
use crate::stats::stats_column::StatsColumn;
use crate::values::names::NameId;

/// A stat, by its place among the stats the mode declares, in the order of their names: what a
/// unit's stat values are kept by. One list, the stat book's, gives every id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatId(u16);

impl StatId {
    /// The stat at `index` among the mode's, within the most stats a mode declares.
    pub(crate) fn new(index: usize) -> StatId {
        StatId(u16::try_from(index).expect("stats fit u16"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for StatId {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> StatId {
        StatId::new(index)
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(StatId, NameKind::Stat, |ctx, name| {
    let stats = ctx
        .view()
        .column(|column: &StatsColumn| column.stat_names().named(name));
    stats.flatten()
});
