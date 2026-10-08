use bevy_ecs::component::Component;

use crate::units::block::Block;
use crate::units::engine_tag::EngineTag;
use crate::units::tag_properties::TagProperties;
use crate::units::tag_set::TagSet;

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
}

#[cfg(test)]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;

    use crate::units::block::Block;
    use crate::units::script_view::View;
    use crate::units::tag_book::TagBook;
    use crate::units::tag_properties::TagProperties;
    use crate::units::unit_tags::UnitTags;
    use crate::units::unit_type::UnitType;

    impl UnitTags {
        /// Gives `entity` the tags of its unit type, as a match's spawn does; one of no type
        /// keeps none.
        pub(crate) fn give_type_tags(world: &mut World, entity: Entity) {
            let Some(&unit_type) = world.get::<UnitType>(entity) else {
                return;
            };
            let tags = match world.get_resource::<TagBook>() {
                Some(book) => book.own(unit_type),
                None => world.non_send::<View>().types_mut().tags(unit_type),
            };
            world.entity_mut(entity).insert(UnitTags {
                tags,
                ..UnitTags::default()
            });
        }

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
