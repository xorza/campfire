use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_sim::{SimWorld, StableId};
use campfire_values::Relations;

/// What the units module gives a world, for every crate.
pub trait UnitsWorld {
    /// The match's entity, once it spawned.
    fn match_entity(&self) -> Option<Entity>;

    /// The relations of the match, on its entity, which every match spawns as it starts.
    fn relations(&self) -> &Relations;

    /// The relations of the match, to change.
    fn relations_mut(&mut self) -> Mut<'_, Relations>;
}

impl UnitsWorld for World {
    fn match_entity(&self) -> Option<Entity> {
        self.entity_of(StableId::MATCH)
    }

    fn relations(&self) -> &Relations {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        self.get(entity)
            .expect("the match's entity holds the relations")
    }

    fn relations_mut(&mut self) -> Mut<'_, Relations> {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        self.get_mut(entity)
            .expect("the match's entity holds the relations")
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;

    use crate::player_units::{AvatarData, AvatarFilter, Commanded};

    /// What the units module gives a test's world.
    pub trait UnitsWorldInternals {
        /// The avatar of `slot`, as `PlayerUnits::avatar` finds it, for code that holds the
        /// world shared, as a test does: its query is made for each read.
        fn avatar_of(&self, slot: PlayerSlot) -> Option<Commanded>;
    }

    impl UnitsWorldInternals for World {
        fn avatar_of(&self, slot: PlayerSlot) -> Option<Commanded> {
            let mut avatars = self.try_query_filtered::<AvatarData, AvatarFilter>()?;
            Commanded::avatar_among(avatars.iter(self), slot)
        }
    }
}
