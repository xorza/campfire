use campfire_script::{NameKind, name_script_arg};
use campfire_values::NameId;

/// A pool, by its place among the pools the mode declares, in the order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PoolId(u8);

impl PoolId {
    pub const FIRST: Self = Self(0);

    /// `None` past `PoolId::LIMIT`.
    pub const fn new(index: u8) -> Option<Self> {
        if index as usize >= <Self as NameId>::LIMIT {
            return None;
        }
        Some(Self(index))
    }

    /// Its place among the mode's pools.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl NameId for PoolId {
    type Scope = ();
    /// The most pools a mode declares, and so a unit holds.
    const LIMIT: usize = 8;

    fn at(index: usize) -> Self {
        Self::new(u8::try_from(index).expect("pools fit u8")).expect("a pool below the limit")
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(PoolId, NameKind::Pool, |ctx, name| {
    ctx.view().names().table::<PoolId>()?.named(name)
});
