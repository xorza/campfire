use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_sim::{SimWorld, StableId};
use campfire_values::Relations;

/// What the units module gives a world, for every crate.
pub trait UnitsWorld {
    /// The match's entity, once it spawned.
    fn match_entity(&self) -> Option<Entity>;

    /// The relations of the match, once its entity spawned, or reached a client.
    fn held_relations(&self) -> Option<&Relations>;
}

impl UnitsWorld for World {
    fn match_entity(&self) -> Option<Entity> {
        self.entity_of(StableId::MATCH)
    }

    fn held_relations(&self) -> Option<&Relations> {
        self.get(self.match_entity()?)
    }
}

/// What the units module gives a world, for its own crate.
pub trait UnitsWorldCrate {
    /// The relations of the match, on its entity, which every match spawns as it
    /// starts.
    fn relations(&self) -> &Relations;

    /// The relations of the match, to change.
    fn relations_mut(&mut self) -> Mut<'_, Relations>;
}

impl UnitsWorldCrate for World {
    fn relations(&self) -> &Relations {
        self.held_relations()
            .expect("a match that started holds its relations")
    }

    fn relations_mut(&mut self) -> Mut<'_, Relations> {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        self.get_mut(entity)
            .expect("the match's entity holds the relations")
    }
}
