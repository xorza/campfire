use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::StateHash;
use campfire_sim::SimWorld;
use tracing::trace;

/// The state hash after each sim tick while the resource exists, from tick 0 when inserted before
/// the match starts: the check the goldens make on every tick, which only a test's build holds.
/// Production hashes only at checkpoints and at the result.
#[derive(Resource, Debug, Default)]
pub struct TickHashes(Vec<StateHash>);

impl TickHashes {
    pub fn get(&self) -> &[StateHash] {
        &self.0
    }

    /// Keeps the state hash of `world` after its tick, while it holds the hashes.
    pub(crate) fn record(world: &mut World) {
        if !world.contains_resource::<Self>() {
            return;
        }
        let hash = world.state_hash();
        let mut hashes = world.resource_mut::<Self>();
        trace!(tick = hashes.0.len(), %hash, "hashed the state");
        hashes.0.push(hash);
    }
}
