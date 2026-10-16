use campfire_values::{Block, Motion, UnitTags};

use crate::action_book::ActionBook;
use crate::action_slots::ActionSlot;
use crate::use_group::UseGroup;

/// What keeps a unit from its actions: its tags, how it moves, and the use group of its slots.
#[derive(Debug, Clone, Copy)]
pub struct Caster<'a> {
    pub tags: UnitTags,
    pub motion: Motion,
    pub used: Option<&'a UseGroup>,
}

impl<'a> Caster<'a> {
    /// The caster of `tags`, none for a unit with no tags, under a forced move when `forced`,
    /// of the use group `used`.
    pub fn of(tags: Option<&UnitTags>, forced: bool, used: Option<&'a UseGroup>) -> Self {
        Self {
            tags: tags.copied().unwrap_or_default(),
            motion: if forced { Motion::Forced } else { Motion::Own },
            used,
        }
    }

    /// Whether it is kept from `block`, as its tags and its motion say.
    pub const fn blocks(self, block: Block) -> bool {
        self.tags.blocks(self.motion, block)
    }

    /// Whether it is kept from the action in `slot`, as `book` gives it: from using it, for a slot
    /// of its use group or a use, else, and with no slot, from casting it.
    pub fn blocked(self, slot: Option<ActionSlot>, book: &ActionBook) -> bool {
        let group = slot.map_or(Block::Cast, |slot| UseGroup::group(self.used, slot, book));
        self.blocks(group)
    }
}
