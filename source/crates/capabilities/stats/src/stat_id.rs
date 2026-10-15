use campfire_script::{NameKind, name_script_arg};
use campfire_values::NameId;
use serde::{Deserialize, Serialize};

/// A stat, by its place among the stats the mode declares, in the order of their names: what a
/// unit's stat values are kept by. One list, the stat book's, gives every id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatId(u16);

impl StatId {
    /// The stat at `index` among the mode's, within the most stats a mode declares.
    pub fn new(index: usize) -> StatId {
        StatId(u16::try_from(index).expect("stats fit u16"))
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
    ctx.view().names().table::<StatId>()?.named(name)
});
