use std::fmt;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::EntityWorldMut;

/// What a capability puts on a player's entity as it spawns.
type PlayerPart = Box<dyn Fn(&mut EntityWorldMut<'_>) + Send + Sync>;

/// What the installed capabilities put on each player's entity as the match starts: each its
/// player state, at its start. Derived from the installs: a restore takes the parts from its
/// snapshot.
#[derive(Resource, Default)]
pub(crate) struct PlayerParts(Vec<PlayerPart>);

impl PlayerParts {
    /// Adds `part`, which each player's entity gets as it spawns.
    pub(crate) fn add(&mut self, part: impl Fn(&mut EntityWorldMut<'_>) + Send + Sync + 'static) {
        self.0.push(Box::new(part));
    }

    /// Puts every part on `player`, in the order the capabilities added them.
    pub(crate) fn put_on(&self, player: &mut EntityWorldMut<'_>) {
        for part in &self.0 {
            part(player);
        }
    }
}

impl fmt::Debug for PlayerParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PlayerParts").field(&self.0.len()).finish()
    }
}
