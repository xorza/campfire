use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_capabilities::Visibility;
use campfire_runner::SessionWorld;
use lightyear::prelude::MessageSender;
use tracing::info;

use crate::match_clock::MatchClock;
use crate::net_protocol::MatchChannel;
use crate::sim_server::error::JoinError;
use crate::sim_server::offering::Joined;
use crate::sim_server::spectators::Spectators;
use crate::sim_server::viewer::Viewer;
use crate::watch_start::WatchStart;

/// What the spectators module gives a world, for its own crate.
pub(crate) trait SpectatorsWorldCrate {
    /// Admits `link` to watch the match, a session of `visibility`, and tells its client the
    /// match's tick; an error when every place is taken.
    fn admit_spectator(&mut self, link: Entity, visibility: &Visibility) -> Result<(), JoinError>;

    /// Tells each spectator's client the match's tick, as the match starts.
    fn start_spectators(&mut self);
}

impl SpectatorsWorldCrate for World {
    fn admit_spectator(&mut self, link: Entity, visibility: &Visibility) -> Result<(), JoinError> {
        let mut viewers = self.query::<&Viewer>();
        let viewer = Spectators::place(visibility, Spectators::count(viewers.iter(self)))?;
        self.entity_mut(link).insert((Joined, viewer));
        tell_start(self, link);
        Ok(())
    }

    fn start_spectators(&mut self) {
        let mut viewers = self.query::<(Entity, &Viewer)>();
        let spectators: Vec<Entity> = viewers
            .iter(self)
            .filter(|(_, viewer)| matches!(viewer, Viewer::Spectator(_)))
            .map(|(link, _)| link)
            .collect();
        for link in spectators {
            tell_start(self, link);
        }
    }
}

/// Tells the client of `link`, a spectator, the Lightyear tick of the match's next sim tick.
fn tell_start(world: &mut World, link: Entity) {
    let first = world.session().log().next_tick();
    let start_tick = world.resource::<MatchClock>().net_tick(first);
    world
        .get_mut::<MessageSender<WatchStart>>(link)
        .expect("a client link sends the watch start")
        .send::<MatchChannel>(WatchStart {
            start_tick: start_tick.0,
            first,
        });
    info!(?link, "a spectator watches");
}
