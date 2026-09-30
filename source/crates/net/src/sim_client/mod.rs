use std::ops::Range;

use bevy_app::{App, FixedUpdate, Plugin, Update};
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::{not, resource_exists};
use bevy_ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{Combat, Control, Navigation, Order, Units};
use campfire_math::SegmentSeed;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SignOnly};
use campfire_protocol::{InputChain, InputHash, PlayerSlot, SessionId};
use campfire_runner::StandInMode;
use campfire_sim::{SimTick, SimUpdate, StateRegistry, TickInput, TickInputs};
use lightyear::prelude::{
    Client, LocalTimeline, MessageReceiver, MessageSender, Tick, is_in_rollback,
};

use crate::input_message::InputMessage;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::net_protocol::InputChannel;

/// A client never holds the segment seed: it predicts movement, never a random outcome.
const PREDICTION_SEED: SegmentSeed = SegmentSeed::new([0; 32]);
/// BIP-340's auxiliary randomness for the chain-head signatures. `net` has no OS randomness yet;
/// without it BIP-340 signs deterministically, which stays secure and gives up only the added
/// hardening against side channels.
const AUX: [u8; 32] = [0; 32];

/// Predicts a match on a Lightyear client: sends the player's orders as chained inputs, signed
/// once per message with the session key, and runs the sim in every fixed tick, rollbacks
/// included, with the player's own inputs.
#[derive(Debug, Clone)]
pub struct SimClient {
    pub session_key: Keypair,
    pub session_id: SessionId,
    /// What the player's first input links to: the id of their delegation.
    pub chain_root: InputHash,
}

/// Orders the player gave, sent in the next fixed tick.
#[derive(Resource, Debug, Default)]
pub struct PendingOrders(Vec<Order>);

impl PendingOrders {
    pub fn push(&mut self, order: Order) {
        self.0.push(order);
    }
}

/// The player's chain, from the match start on, and the inputs sent by stamp, to predict with
/// again after a rollback. Stamps never decrease, so a tick's inputs are one run.
#[derive(Resource, Debug)]
struct SentInputs {
    client: SimClient,
    secp: Secp256k1<SignOnly>,
    chain: Option<InputChain>,
    inputs: Vec<SentInput>,
    payloads: Vec<u8>,
}

#[derive(Debug)]
struct SentInput {
    stamp: u64,
    payload: Range<usize>,
}

impl SentInputs {
    fn at(&self, stamp: u64) -> impl Iterator<Item = &[u8]> {
        let start = self.inputs.partition_point(|input| input.stamp < stamp);
        self.inputs[start..]
            .iter()
            .take_while(move |input| input.stamp == stamp)
            .map(|input| &self.payloads[input.payload.clone()])
    }
}

impl Plugin for SimClient {
    fn build(&self, app: &mut App) {
        let world = app.world_mut();
        SimUpdate::prepare(world, PREDICTION_SEED);
        let mut schedule = SimUpdate::schedule();
        // A client hashes no state, so the registry the capabilities fill is not kept.
        let mut state = StateRegistry::new();
        Units::install(
            world,
            &mut schedule,
            &mut state,
            StandInMode::SCRIPT_LIMITS,
            StandInMode::TICK_RATE,
        );
        Combat::install(world, &mut schedule, &mut state);
        Navigation::install(world, &mut schedule, &mut state);
        Control::install(world, &mut schedule, &mut state);
        world.add_schedule(schedule);
        world.insert_resource(SentInputs {
            client: self.clone(),
            secp: Secp256k1::signing_only(),
            chain: None,
            inputs: Vec::new(),
            payloads: Vec::new(),
        });
        app.init_resource::<PendingOrders>();
        app.add_systems(Update, receive_match_start);
        app.add_systems(
            FixedUpdate,
            (send_orders.run_if(not(is_in_rollback)), run_predicted_tick)
                .chain()
                .run_if(resource_exists::<MatchClock>),
        );
    }
}

fn receive_match_start(
    mut receivers: Query<'_, '_, &mut MessageReceiver<MatchStart>, With<Client>>,
    mut sent: ResMut<'_, SentInputs>,
    mut commands: Commands<'_, '_>,
) {
    for mut receiver in &mut receivers {
        for start in receiver.receive() {
            sent.chain = Some(InputChain::new(
                PlayerSlot::new(start.slot),
                sent.client.chain_root,
            ));
            commands.insert_resource(MatchClock::new(Tick(start.start_tick)));
        }
    }
}

/// Stamps each pending order with the sim tick about to run, chains it and keeps it, then sends
/// them all in one message signed over the chain head after the last.
fn send_orders(
    timeline: Res<'_, LocalTimeline>,
    clock: Res<'_, MatchClock>,
    mut pending: ResMut<'_, PendingOrders>,
    mut sent: ResMut<'_, SentInputs>,
    mut sender: Single<'_, '_, &mut MessageSender<InputMessage>, With<Client>>,
) {
    let SentInputs {
        client,
        secp,
        chain: Some(chain),
        inputs,
        payloads,
    } = &mut *sent
    else {
        return;
    };
    let Some(stamp) = clock.sim_tick(timeline.tick()) else {
        return;
    };
    if pending.0.is_empty() {
        return;
    }
    let first = inputs.len();
    for order in pending.0.drain(..) {
        let start = payloads.len();
        payloads.extend_from_slice(&Order::payload(&[order]));
        inputs.push(SentInput {
            stamp,
            payload: start..payloads.len(),
        });
    }
    let sent = &inputs[first..];
    // The signature goes in the message with the inputs, so a copy of the chain reaches the head
    // first.
    let mut head = *chain;
    for input in sent {
        head.extend(stamp, &payloads[input.payload.clone()]);
    }
    let signature = head.sign(secp, &client.session_key, client.session_id, &AUX);
    let chained = sent
        .iter()
        .map(|input| chain.extend(stamp, &payloads[input.payload.clone()]));
    sender.send::<InputChannel>(InputMessage::new(chained, signature));
    debug_assert_eq!(*chain, head, "the message's inputs end at the signed head");
}

/// Runs the sim tick of the current Lightyear tick with the player's inputs stamped for it; in a
/// rollback, Lightyear has wound its timeline back, so the sim follows.
fn run_predicted_tick(world: &mut World) {
    let tick = world.resource::<LocalTimeline>().tick();
    let Some(sim_tick) = world.resource::<MatchClock>().sim_tick(tick) else {
        return;
    };
    let Some(slot) = world
        .resource::<SentInputs>()
        .chain
        .map(|chain| chain.slot())
    else {
        return;
    };
    *world.resource_mut::<SimTick>() = SimTick::new(sim_tick);
    world.resource_scope(|world, sent: Mut<'_, SentInputs>| {
        let mut inputs = world.resource_mut::<TickInputs>();
        for payload in sent.at(sim_tick) {
            inputs.push(TickInput {
                slot: slot.get(),
                payload,
            });
        }
    });
    world.run_schedule(SimUpdate);
}

#[cfg(feature = "bench")]
pub(crate) mod bench;
