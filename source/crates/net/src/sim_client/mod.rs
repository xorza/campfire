use std::ops::Range;

use bevy_app::{App, FixedUpdate, Plugin, Update};
use bevy_ecs::query::{Added, Allow, Has, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::{not, resource_exists};
use bevy_ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{Dead, Order, Owner};
use campfire_log::LogEvent;
use campfire_math::SegmentSeed;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SignOnly};
use campfire_protocol::{Delegation, DelegationTerms, InputChain, InputHash, SessionId};
use campfire_sim::{
    SimTick, SimUpdate, StableId, StateRegistry, Tick, TickInput, TickInputs, TickRate,
};
use lightyear::prelude::client::{InputDelayConfig, InputTimelineConfig};
use lightyear::prelude::{
    Client, LocalTimeline, MessageReceiver, MessageSender, Predicted, SyncConfig, Tick as NetTick,
    is_in_rollback,
};
use tracing::{debug, info, warn};

use crate::error::TermsMismatch;
use crate::events::match_started::MatchStarted;
use crate::events::orders_sent::OrdersSent;
use crate::input_message::InputMessage;
use crate::join::Join;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::net_protocol::{InputChannel, JoinChannel};
use crate::offer::Offer;
use crate::sim_client::bot_script::BotScript;
use crate::sim_client::client_mode::ClientMode;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::unpredicted::Unpredicted;

pub(crate) mod bot_script;
pub(crate) mod client_mode;
pub(crate) mod server_pin;
pub(crate) mod unpredicted;

/// A client never holds the segment seed: it predicts movement, never a random outcome.
const PREDICTION_SEED: SegmentSeed = SegmentSeed::new([0; 32]);
/// How long a delegation lets the session key sign, in seconds: a day, longer than a LAN match.
const DELEGATION_LIFETIME: u64 = 86_400;

/// Plays a session on a Lightyear client: answers the server's offer with a delegation of a
/// session key, sends the player's orders as chained inputs, signed once per message with the
/// session key, and runs the sim in every fixed tick, rollbacks included, with the player's own
/// inputs, on the units the client predicts.
#[derive(Debug, Clone)]
pub struct SimClient {
    /// The player's Nostr identity, which signs the delegation.
    pub main_key: Keypair,
    /// This session's key, which the delegation lets sign the player's inputs.
    pub session_key: Keypair,
    pub server: ServerPin,
    pub mode: ClientMode,
    /// Unix seconds: when the delegation is made, and so when it expires.
    pub clock: fn() -> u64,
    /// Fills a seed contribution or BIP-340's auxiliary randomness with random bytes.
    pub entropy: fn(&mut [u8; 32]),
}

/// Where the client is in joining the session.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinState {
    /// No offer came yet.
    Waiting,
    /// The offer named a session the client cannot play; it did not answer.
    Refused(TermsMismatch),
    /// It answered the offer, and waits for the match to start.
    Joined,
}

/// Orders the player gave, sent in the next fixed tick.
#[derive(Resource, Debug, Default)]
pub struct PendingOrders(Vec<Order>);

impl PendingOrders {
    pub fn push(&mut self, order: Order) {
        self.0.push(order);
    }
}

/// The player's session once they joined, their chain from the match start on, and the inputs
/// sent by stamp, to predict with again after a rollback. Stamps never decrease, so a tick's
/// inputs are one run.
#[derive(Resource, Debug)]
struct SentInputs {
    client: SimClient,
    secp: Secp256k1<SignOnly>,
    session: Option<JoinedSession>,
    chain: Option<InputChain>,
    inputs: Vec<SentInput>,
    payloads: Vec<u8>,
}

/// What the player's join fixed: the session its signatures name, and what their first input
/// links to, their delegation's id.
#[derive(Debug, Clone, Copy)]
struct JoinedSession {
    id: SessionId,
    chain_root: InputHash,
}

#[derive(Debug)]
struct SentInput {
    stamp: Tick,
    payload: Range<usize>,
}

impl SentInputs {
    fn at(&self, stamp: Tick) -> impl Iterator<Item = &[u8]> {
        let start = self.inputs.partition_point(|input| input.stamp < stamp);
        self.inputs[start..]
            .iter()
            .take_while(move |input| input.stamp == stamp)
            .map(|input| &self.payloads[input.payload.clone()])
    }

    /// The player's answer to `offer`: a delegation of the session key in the offered session,
    /// with a fresh seed contribution, and the session key's signature over the challenge and
    /// the pinned certificate hash. An error when the terms name another server, or a session
    /// the client cannot play.
    fn join(&mut self, offer: &Offer) -> Result<Join, TermsMismatch> {
        let client = &self.client;
        if offer.terms.server_key != client.server.key {
            return Err(TermsMismatch::OtherServer);
        }
        client.mode.fits(&offer.terms)?;
        let mut seed_contribution = [0; 32];
        (client.entropy)(&mut seed_contribution);
        let now = (client.clock)();
        let id = offer.terms.session_id();
        let granted = DelegationTerms {
            session_key: client.session_key.x_only_public_key().0,
            server_key: offer.terms.server_key,
            session_id: id,
            seed_contribution,
            expiration: now + DELEGATION_LIFETIME,
        };
        let mut aux = [0; 32];
        (client.entropy)(&mut aux);
        let delegation = Delegation::sign(&self.secp, &client.main_key, &granted, now, &aux);
        (client.entropy)(&mut aux);
        let answer = offer.challenge.answer(
            &self.secp,
            &client.session_key,
            &client.server.certificate,
            &aux,
        );
        self.session = Some(JoinedSession {
            id,
            chain_root: delegation.chain_root(),
        });
        Ok(Join {
            delegation: delegation.json().to_owned(),
            answer,
        })
    }
}

impl Plugin for SimClient {
    fn build(&self, app: &mut App) {
        let world = app.world_mut();
        let rate = TickRate::new(self.mode.tick_hz);
        SimUpdate::prepare(world, PREDICTION_SEED, rate);
        let mut schedule = SimUpdate::schedule();
        // A client hashes no state, so the registry the capabilities fill is not kept.
        let mut state = StateRegistry::new();
        // A client runs no scripts: it predicts only its own player's units.
        self.mode
            .capabilities
            .install(world, &mut schedule, &mut state, None);
        world.insert_resource(self.mode.bounds);
        world.add_schedule(schedule);
        Unpredicted::install(world);
        world.insert_resource(SentInputs {
            client: self.clone(),
            secp: Secp256k1::signing_only(),
            session: None,
            chain: None,
            inputs: Vec::new(),
            payloads: Vec::new(),
        });
        // Prediction covers all the latency, with no input delay: an input goes out stamped with the
        // tick the client predicts it in, which Lightyear keeps ahead of the server's by the
        // round trip; with input delay it would keep that tick nearer, and the input would land
        // late.
        app.insert_resource(InputTimelineConfig::new(
            SyncConfig::default(),
            InputDelayConfig::no_input_delay(),
        ));
        app.insert_resource(JoinState::Waiting);
        app.init_resource::<PendingOrders>();
        app.add_systems(
            Update,
            (answer_offer, receive_match_start, report_deaths).chain(),
        );
        app.add_systems(
            FixedUpdate,
            (send_orders.run_if(not(is_in_rollback)), run_predicted_tick)
                .chain()
                .run_if(resource_exists::<MatchClock>),
        );
    }
}

/// Answers the first offer, unless its terms do not fit.
fn answer_offer(
    mut receivers: Query<'_, '_, &mut MessageReceiver<Offer>, With<Client>>,
    mut sender: Single<'_, '_, &mut MessageSender<Join>, With<Client>>,
    mut sent: ResMut<'_, SentInputs>,
    mut state: ResMut<'_, JoinState>,
) {
    for mut receiver in &mut receivers {
        for offer in receiver.receive() {
            if *state != JoinState::Waiting {
                continue;
            }
            let session = offer.terms.session_id();
            *state = match sent.join(&offer) {
                Ok(join) => {
                    sender.send::<JoinChannel>(join);
                    info!(%session, "joined the offered session");
                    JoinState::Joined
                }
                Err(mismatch) => {
                    warn!(%session, %mismatch, "did not join the offered session");
                    JoinState::Refused(mismatch)
                }
            };
        }
    }
}

fn receive_match_start(
    mut receivers: Query<'_, '_, &mut MessageReceiver<MatchStart>, With<Client>>,
    mut sent: ResMut<'_, SentInputs>,
    mut commands: Commands<'_, '_>,
) {
    for mut receiver in &mut receivers {
        for start in receiver.receive() {
            let Some(session) = sent.session else {
                continue;
            };
            sent.chain = Some(InputChain::new(start.slot, session.chain_root));
            MatchStarted {
                slot: start.slot,
                start_tick: start.start_tick,
            }
            .log();
            commands.insert_resource(MatchClock::new(NetTick(start.start_tick)));
        }
    }
}

/// The units whose death the server's state just brought, and whether each is the client's own.
type Died<'w, 's> =
    Query<'w, 's, (&'static StableId, Has<Predicted>), (Added<Dead>, Allow<Unpredicted>)>;

/// Logs each death the server's state brings, at the client's own tick, which runs ahead of the
/// server's.
fn report_deaths(
    timeline: Res<'_, LocalTimeline>,
    clock: Option<Res<'_, MatchClock>>,
    died: Died<'_, '_>,
) {
    let tick = clock.and_then(|clock| clock.sim_tick(timeline.tick()));
    for (id, own) in &died {
        let unit = id.get();
        let tick = tick.map(Tick::get);
        if own {
            info!(unit, tick, "learned that the player's own unit died");
        } else {
            debug!(unit, tick, "learned that a unit died");
        }
    }
}

/// The client's own avatar: the one unit it predicts under a player's control.
type OwnAvatar<'w, 's> = Query<'w, 's, &'static StableId, (With<Owner>, With<Predicted>)>;

/// Adds the bot script's orders due in the tick about to run, for the player's own avatar, then
/// stamps each pending order with that tick, chains it and keeps it, and sends them all in one
/// message signed over the chain head after the last.
fn send_orders(
    timeline: Res<'_, LocalTimeline>,
    clock: Res<'_, MatchClock>,
    bot: Option<ResMut<'_, BotScript>>,
    avatar: OwnAvatar<'_, '_>,
    mut pending: ResMut<'_, PendingOrders>,
    mut sent: ResMut<'_, SentInputs>,
    mut sender: Single<'_, '_, &mut MessageSender<InputMessage>, With<Client>>,
) {
    let SentInputs {
        client,
        secp,
        session: Some(session),
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
    if let (Some(mut bot), Ok(&unit)) = (bot, avatar.single()) {
        for scripted in bot.due(stamp) {
            pending.push(Order {
                unit,
                action: scripted.action,
            });
        }
    }
    if pending.0.is_empty() {
        return;
    }
    OrdersSent {
        stamp,
        orders: pending.0.len(),
    }
    .log();
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
        head.extend(stamp.get(), &payloads[input.payload.clone()]);
    }
    let mut aux = [0; 32];
    (client.entropy)(&mut aux);
    let signature = head.sign(secp, &client.session_key, session.id, &aux);
    let chained = sent
        .iter()
        .map(|input| chain.extend(stamp.get(), &payloads[input.payload.clone()]));
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
            inputs.push(TickInput { slot, payload });
        }
    });
    world.run_schedule(SimUpdate);
}

#[cfg(feature = "bench")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
