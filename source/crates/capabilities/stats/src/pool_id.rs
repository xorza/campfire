use campfire_core::{NameKind, name_script_arg};
use campfire_values::NameId;

use crate::stats_column::StatsColumn;

/// A pool, by its place among the pools the mode declares, in the order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PoolId(u8);

impl PoolId {
    pub const FIRST: PoolId = PoolId(0);

    /// The most pools a mode declares, and so a unit holds.
    pub const LIMIT: usize = 8;

    /// `None` past `PoolId::LIMIT`.
    pub const fn new(index: u8) -> Option<PoolId> {
        if index as usize >= PoolId::LIMIT {
            return None;
        }
        Some(PoolId(index))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for PoolId {
    type Scope = ();
    const LIMIT: usize = PoolId::LIMIT;

    fn at(index: usize) -> PoolId {
        PoolId::new(u8::try_from(index).expect("pools fit u8")).expect("a pool below the limit")
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(PoolId, NameKind::Pool, |ctx, name| {
    let pools = ctx
        .view()
        .column(|column: &StatsColumn| column.pool_names().named(name));
    pools.flatten()
});
