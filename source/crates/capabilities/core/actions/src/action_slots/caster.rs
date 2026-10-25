use campfire_values::{Block, Motion, UnitTags};

use crate::action_book::ActionBook;
use crate::action_slots::ActionSlot;
use crate::action_slots::use_group::UseGroup;

/// What keeps a unit from its actions: its tags, how it moves, and the use group of its slots.
#[derive(Debug, Clone, Copy)]
pub struct Caster<'a> {
    pub tags: UnitTags,
    pub motion: Motion,
    pub used: Option<&'a UseGroup>,
}

impl<'a> Caster<'a> {
    /// The caster of `tags`, none for a unit with no tags, in `motion`, of the use group `used`.
    pub fn of(tags: Option<&UnitTags>, motion: Motion, used: Option<&'a UseGroup>) -> Self {
        Self {
            tags: tags.copied().unwrap_or_default(),
            motion,
            used,
        }
    }

    /// Whether it is kept from `block`, as its tags and its motion say.
    pub const fn blocks(self, block: Block) -> bool {
        self.tags.blocks(self.motion, block)
    }

    /// Whether it is kept from the action in `slot`, as `book` gives it: from using it, for a slot
    /// of its use group or a use, else, and with no slot, from casting it; and from an action
    /// whose `when` its tags do not pass (04-capabilities/actions.md, An action may need tags).
    pub fn blocked(self, slot: Option<ActionSlot>, book: &ActionBook) -> bool {
        let action = slot.and_then(|slot| book.get(slot.action?));
        let unmet = action.is_some_and(|action| !action.usable_by(self.tags.tags));
        let group = slot.map_or(Block::Cast, |slot| UseGroup::group(self.used, slot, book));
        unmet || self.blocks(group)
    }
}
