use bevy_ecs::component::Component;

use crate::units::tag_effects::TagEffects;
use crate::units::tag_set::TagSet;

/// A unit's tags, their effects, and the tags it is immune to: derived from its type and its
/// modifiers whenever they change, never state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UnitTags {
    pub(crate) tags: TagSet,
    pub(crate) effects: TagEffects,
    pub(crate) immune: TagSet,
}

impl UnitTags {
    /// The effects of a unit with `tags`: none for one with no tags.
    pub(crate) fn effects_of(tags: Option<&UnitTags>) -> TagEffects {
        tags.map_or_else(TagEffects::default, |tags| tags.effects)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;

    use crate::units::block::Block;
    use crate::units::script_view::View;
    use crate::units::tag_effects::TagEffects;
    use crate::units::unit_tags::UnitTags;
    use crate::units::unit_type::UnitType;

    impl UnitTags {
        /// Gives `entity` the tags of its unit type, as a match's spawn does; one of no type
        /// keeps none.
        pub(crate) fn give_type_tags(world: &mut World, entity: Entity) {
            let Some(&unit_type) = world.get::<UnitType>(entity) else {
                return;
            };
            let tags = world.non_send::<View>().types_mut().tags(unit_type);
            world.entity_mut(entity).insert(UnitTags {
                tags,
                ..UnitTags::default()
            });
        }

        /// Tags of no name whose effects block `blocks`, as a unit's tags would.
        pub(crate) fn blocking(blocks: &[Block]) -> UnitTags {
            let effects = blocks
                .iter()
                .fold(TagEffects::default(), |effects, &block| {
                    effects.with_block(block)
                });
            UnitTags::with_effects(effects)
        }

        /// Tags of no name with `effects`.
        pub(crate) fn with_effects(effects: TagEffects) -> UnitTags {
            UnitTags {
                effects,
                ..UnitTags::default()
            }
        }
    }
}
