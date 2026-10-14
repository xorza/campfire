use bevy_ecs::system::{Res, SystemParam};
use campfire_units::TagBook;

use crate::modifier_book::ModifierBook;
use crate::pool_book::PoolBook;
use crate::stat_book::StatBook;

/// The books a system derives stats, pools and tags from: the stat and pool books, empty before
/// a mode loads them, the modifiers, and the tags, empty in a match that declares no tag.
#[derive(SystemParam, Debug)]
pub(crate) struct StatBooks<'w> {
    pub(crate) stats: Res<'w, StatBook>,
    pub(crate) pools: Res<'w, PoolBook>,
    pub modifiers: Res<'w, ModifierBook>,
    pub tags: Res<'w, TagBook>,
}
