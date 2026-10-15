use bevy_ecs::bundle::Bundle;
use bevy_ecs::system::EntityCommands;
use bevy_ecs::world::EntityWorldMut;

/// Where a new unit's parts go: its entity in the world, or by commands.
pub trait SpawnTarget {
    fn insert_part<B: Bundle>(&mut self, part: B);
}

impl SpawnTarget for EntityWorldMut<'_> {
    fn insert_part<B: Bundle>(&mut self, part: B) {
        self.insert(part);
    }
}

impl SpawnTarget for EntityCommands<'_> {
    fn insert_part<B: Bundle>(&mut self, part: B) {
        self.insert(part);
    }
}
