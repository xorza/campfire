use bevy_ecs::world::{Mut, World};

use crate::players::players_world::PlayersWorld;
use crate::units::relations::Relations;

/// What the units module gives a world, for every crate.
pub trait UnitsWorld {
    /// The relations of the match, once its entity spawned, or reached a client.
    fn held_relations(&self) -> Option<&Relations>;
}

impl UnitsWorld for World {
    fn held_relations(&self) -> Option<&Relations> {
        self.get(self.match_entity()?)
    }
}

/// What the units module gives a world, for its own crate.
pub(crate) trait UnitsWorldCrate {
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
