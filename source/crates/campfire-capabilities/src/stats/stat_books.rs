use bevy_ecs::system::{Res, SystemParam};

use crate::stats::modifier_book::ModifierBook;
use crate::stats::pool_book::PoolBook;
use crate::stats::stat_book::StatBook;
use crate::units::tag_book::TagBook;

/// The books a system derives stats, pools and tags from: the stat and pool books, empty before
/// a mode loads them, the modifiers, and the tags, empty in a match that declares no tag.
#[derive(SystemParam, Debug)]
pub(crate) struct StatBooks<'w> {
    pub(crate) stats: Res<'w, StatBook>,
    pub(crate) pools: Res<'w, PoolBook>,
    pub(crate) modifiers: Res<'w, ModifierBook>,
    pub(crate) tags: Res<'w, TagBook>,
}
