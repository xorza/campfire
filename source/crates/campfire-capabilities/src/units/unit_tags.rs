use std::iter;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Commands, Query, Res};

use crate::units::block::Block;
use crate::units::delivery_entity::DeliveryEntity;
use crate::units::engine_tag::EngineTag;
use crate::units::tag_book::TagBook;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;
use crate::units::unit_type::UnitType;

/// A unit's tags, their properties, and the tags it is immune to: derived from its type and its
/// modifiers whenever they change, never state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UnitTags {
    pub(crate) tags: TagSet,
    pub(crate) properties: TagProperties,
    pub(crate) immune: TagSet,
}

impl UnitTags {
    /// The properties of a unit with `tags`: none for one with no tags.
    pub(crate) fn properties_of(tags: Option<&UnitTags>) -> TagProperties {
        tags.map_or_else(TagProperties::default, |tags| tags.properties)
    }

    /// Whether a unit with `tags` gathers: it has the engine's `gathering` tag, which a gather
    /// under way gives it.
    pub(crate) fn gathers(tags: Option<&UnitTags>) -> bool {
        tags.is_some_and(|tags| tags.tags.contains(EngineTag::Gathering.tag()))
    }

    /// Whether a unit with `tags`, under a forced move when `forced`, is kept from `block`: by its
    /// tags, or by the forced move from a step, a cast or an attack.
    pub(crate) fn blocks(tags: Option<&UnitTags>, forced: bool, block: Block) -> bool {
        UnitTags::properties_of(tags).blocks(block)
            || forced && matches!(block, Block::Move | Block::Cast | Block::Attack)
    }

    /// The tags a unit of `unit_type` spawns with: its type's own, from `book`, none in a match
    /// with no tag book. Its modifiers' grants join them as stats derive them.
    pub(crate) fn at_spawn(book: Option<&TagBook>, unit_type: UnitType) -> UnitTags {
        book.map_or(UnitTags::default(), |book| {
            book.unit_tags(unit_type, iter::empty())
        })
    }

    /// Gives each delivery that lacks them, as one a restore brings back, the tags it spawned
    /// with, as `UnitCore` gives them. A delivery holds no modifier to change them.
    pub(crate) fn give_delivery_tags(
        book: Option<Res<'_, TagBook>>,
        deliveries: Query<'_, '_, (Entity, &UnitType), (With<DeliveryEntity>, Without<UnitTags>)>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, &unit_type) in &deliveries {
            let tags = UnitTags::at_spawn(book.as_deref(), unit_type);
            commands.entity(entity).insert(tags);
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {

    use crate::units::block::Block;
    use crate::units::tag_properties::TagProperties;
    use crate::units::unit_tags::UnitTags;

    impl UnitTags {
        /// Tags of no name whose properties block `blocks`, as a unit's tags would.
        pub(crate) fn blocking(blocks: &[Block]) -> UnitTags {
            let properties = blocks
                .iter()
                .fold(TagProperties::default(), |properties, &block| {
                    properties.with_block(block)
                });
            UnitTags::with_properties(properties)
        }

        /// Tags of no name with `properties`.
        pub(crate) fn with_properties(properties: TagProperties) -> UnitTags {
            UnitTags {
                properties,
                ..UnitTags::default()
            }
        }
    }
}
