use bevy_app::{App, FixedUpdate, Plugin};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::system::{Local, Query, ResMut};
use bevy_ecs::world::World;
use campfire_capabilities::Controller;
use campfire_protocol::{Applied, PlayerSlot, ServerSeed, SessionLog};
use campfire_runner::{Session, StartError};
use campfire_sim::{EntityIndex, SimTick, StateHash};
use lightyear::core::tick::TickDuration;
use lightyear::prelude::{
    LocalTimeline, MessageReceiver, MessageSender, NetworkTarget, PredictionTarget, Replicate,
};

use crate::input_message::InputMessage;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::net_protocol::MatchChannel;

/// Runs a match on a Lightyear server: records the packets players send, runs one sim tick in each
/// fixed tick, and keeps the state hash after each.
#[derive(Debug)]
pub struct SimServer;

/// Which player a client link carries the inputs of, and how many of its messages the log refused.
#[derive(Component, Debug, Clone, Copy)]
pub struct PlayerLink {
    slot: PlayerSlot,
    refused: u64,
}

impl PlayerLink {
    /// Messages the log refused: a broken chain or signature, or a limit passed. An honest
    /// client sends none; with the connect handshake, the first one will end the connection.
    pub const fn refused(self) -> u64 {
        self.refused
    }
}

/// The state hash after each sim tick, from tick 0.
#[derive(Resource, Debug, Default)]
pub struct TickHashes(Vec<StateHash>);

impl TickHashes {
    pub fn get(&self) -> &[StateHash] {
        &self.0
    }
}

impl Plugin for SimServer {
    fn build(&self, app: &mut App) {
        app.init_resource::<TickHashes>();
        app.add_systems(
            FixedUpdate,
            (record_inputs, run_sim_tick)
                .chain()
                .run_if(resource_exists::<MatchClock>),
        );
    }
}

impl SimServer {
    /// Starts the match of `log`'s header in the next fixed tick, recording into `log`. `clients`
    /// are the links of the players, by slot; each learns its slot and the start tick, and every
    /// hero replicates to every client, predicted. Towers and creeps do not replicate yet.
    pub fn start_match(
        world: &mut World,
        log: SessionLog,
        server_seed: ServerSeed,
        clients: &[Entity],
    ) -> Result<(), StartError> {
        assert_eq!(
            clients.len(),
            log.header().players.len(),
            "one client per player"
        );
        assert_eq!(
            world.resource::<TickDuration>().0,
            log.header().terms.tick_length(),
            "the server ticks at the session's rate"
        );
        Session::start(world, log, server_seed)?;
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::new(start));

        let heroes: Vec<Entity> = world
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, unit)| unit)
            .filter(|&unit| world.entity(unit).contains::<Controller>())
            .collect();
        for hero in heroes {
            world.entity_mut(hero).insert((
                Replicate::to_clients(NetworkTarget::All),
                PredictionTarget::to_clients(NetworkTarget::All),
            ));
        }
        for (slot, &client) in (0..).zip(clients) {
            world.entity_mut(client).insert(PlayerLink {
                slot: PlayerSlot::new(slot),
                refused: 0,
            });
            world
                .get_mut::<MessageSender<MatchStart>>(client)
                .expect("a client link sends the match start")
                .send::<MatchChannel>(MatchStart {
                    start_tick: start.0,
                    slot,
                });
        }
        Ok(())
    }
}

/// Logs each received packet before the tick about to run.
fn record_inputs(
    mut links: Query<'_, '_, (&mut PlayerLink, &mut MessageReceiver<InputMessage>)>,
    mut session: ResMut<'_, Session>,
    mut applied: Local<'_, Vec<Applied>>,
) {
    for (mut link, mut receiver) in &mut links {
        for message in receiver.receive() {
            let recorded = message.inputs(link.slot).is_some_and(|inputs| {
                session
                    .record(inputs, message.signature(), &mut applied)
                    .is_ok()
            });
            if !recorded {
                link.refused += 1;
            }
        }
    }
}

fn run_sim_tick(world: &mut World) {
    let tick = world.resource::<LocalTimeline>().tick();
    let Some(sim_tick) = world.resource::<MatchClock>().sim_tick(tick) else {
        return;
    };
    debug_assert_eq!(
        sim_tick,
        world.resource::<SimTick>().get(),
        "the server runs every sim tick once, in order"
    );
    Session::run_tick(world);
    let hash = world.resource::<Session>().state_hash(world);
    world.resource_mut::<TickHashes>().0.push(hash);
}
