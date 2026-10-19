use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_runner::SessionWorld;
use lightyear::prelude::MessageSender;

use crate::match_clock::MatchClock;
use crate::net_protocol::watch_start::WatchStart;
use crate::sim_server::spectators::Spectators;
use crate::sim_server::viewer::Viewer;

/// What the spectators module gives a world, for its own crate.
pub(crate) trait SpectatorsWorld {
    /// Tells each spectator's client the match's tick, as the match starts.
    fn start_spectators(&mut self);
}

impl SpectatorsWorld for World {
    fn start_spectators(&mut self) {
        let mut viewers = self.query::<(Entity, &Viewer)>();
        let spectators: Vec<Entity> = viewers
            .iter(self)
            .filter(|(_, viewer)| matches!(viewer, Viewer::Spectator(_)))
            .map(|(link, _)| link)
            .collect();
        let first = self.session().log().next_tick();
        let clock = *self.resource::<MatchClock>();
        for link in spectators {
            let mut sender = self
                .get_mut::<MessageSender<WatchStart>>(link)
                .expect("a client link sends the watch start");
            Spectators::tell_start(&mut sender, &clock, first, link);
        }
    }
}
