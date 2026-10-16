use bevy_ecs::component::Component;

use crate::block::Block;
use crate::tag_properties::TagProperties;
use crate::tag_set::TagSet;

/// A unit's tags, their properties, and the tags it is immune to: derived from its type and its
/// modifiers whenever they change, never state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnitTags {
    pub tags: TagSet,
    pub properties: TagProperties,
    pub immune: TagSet,
}

/// How a unit moves: by its own steps, or by force, as a knockback moves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Own,
    Forced,
}

impl UnitTags {
    /// Whether a unit with these tags passes through each unit that passes too, as a worker in
    /// its gather loop does.
    pub const fn passes(self) -> bool {
        self.properties.passes()
    }

    /// Whether a unit with these tags, in `motion`, is kept from `block`: by its tags, or by a
    /// forced move from a step, a cast or an attack.
    pub const fn blocks(self, motion: Motion, block: Block) -> bool {
        self.properties.blocks(block)
            || matches!(motion, Motion::Forced)
                && matches!(block, Block::Move | Block::Cast | Block::Attack)
    }
}
