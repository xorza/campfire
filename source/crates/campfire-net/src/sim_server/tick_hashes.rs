use bevy_ecs::resource::Resource;
use campfire_common::StateHash;

/// The state hash after each sim tick while the resource exists, from tick 0 when inserted before
/// the match starts: the check the goldens make on every tick, for tests and for a host that
/// looks for a divergence. Production hashes only at checkpoints and at the result.
#[derive(Resource, Debug, Default)]
pub struct TickHashes(Vec<StateHash>);

impl TickHashes {
    pub fn get(&self) -> &[StateHash] {
        &self.0
    }

    pub(crate) fn push(&mut self, hash: StateHash) {
        self.0.push(hash);
    }
}
