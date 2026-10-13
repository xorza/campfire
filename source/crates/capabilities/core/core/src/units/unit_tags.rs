use std::iter;

use bevy_ecs::component::Component;

use crate::units::block::Block;
use crate::units::tag_book::TagBook;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;
use crate::units::unit_type::UnitType;

/// A unit's tags, their properties, and the tags it is immune to: derived from its type and its
/// modifiers whenever they change, never state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnitTags {
    pub tags: TagSet,
    pub properties: TagProperties,
    pub immune: TagSet,
}

impl UnitTags {
    /// The properties of a unit with `tags`: none for one with no tags.
    pub fn properties_of(tags: Option<&UnitTags>) -> TagProperties {
        tags.map_or_else(TagProperties::default, |tags| tags.properties)
    }

    /// Whether a unit with `tags` passes through each unit that passes too, as a worker in its
    /// gather loop does.
    pub fn passes(tags: Option<&UnitTags>) -> bool {
        UnitTags::properties_of(tags).passes()
    }

    /// Whether a unit with `tags`, under a forced move when `forced`, is kept from `block`: by its
    /// tags, or by the forced move from a step, a cast or an attack.
    pub fn blocks(tags: Option<&UnitTags>, forced: bool, block: Block) -> bool {
        UnitTags::properties_of(tags).blocks(block)
            || forced && matches!(block, Block::Move | Block::Cast | Block::Attack)
    }

    /// The tags a unit of `unit_type` spawns with: its type's own, from `book`, none in a world
    /// with no tag book, as a copy of a match's state. Its modifiers' grants join them as stats
    /// derive them.
    pub fn at_spawn(book: Option<&TagBook>, unit_type: UnitType) -> UnitTags {
        book.map_or_else(UnitTags::default, |book| {
            book.unit_tags(unit_type, iter::empty())
        })
    }
}
