use std::collections::BTreeMap;

use crate::stats::pool_data::PoolData;
use crate::values::declared_name::DeclaredName;

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

    /// The pool `name` among the mode's `pools`; `None` when the mode does not declare it, or
    /// it is past the limit.
    pub fn named(pools: &BTreeMap<DeclaredName, PoolData>, name: &str) -> Option<PoolId> {
        let at = pools.keys().position(|pool| pool.as_str() == name)?;
        PoolId::new(u8::try_from(at).ok()?)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
