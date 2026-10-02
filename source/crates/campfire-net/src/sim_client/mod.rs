use std::sync::Arc;

use bevy_app::{App, FixedUpdate, Plugin, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Added, Allow, Has, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::not;
use bevy_ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{Dead, MatchEnd, Order, Owner, Relations};
use campfire_common::{SegmentSeed, Tick};
use campfire_log::LogEvent;
use campfire_package::ModePackages;
use campfire_protocol::PlayerInput;
use campfire_protocol::secp256k1::Keypair;
use campfire_runner::SessionRules;
use campfire_sim::{
    SimTick, SimUpdate, StableId, StateRegistry, TickInput, TickInputs, TickRate, Unpredicted,
};
use lightyear::prelude::client::{InputDelayConfig, InputTimelineConfig};
use lightyear::prelude::{
    Client, Disconnect, LocalTimeline, MessageReceiver, MessageSender, Predicted,
    PredictionManager, Replicated, SyncConfig, is_in_rollback,
};
use tracing::{debug, info, warn};

use crate::events::match_started::MatchStarted;
use crate::events::orders_sent::OrdersSent;
use crate::input_message::InputMessage;
use crate::join::Join;
use crate::match_start::MatchStart;
use crate::net_protocol::{InputChannel, JoinChannel};
use crate::offer::Offer;
use crate::sim_client::bot_script::BotScript;
use crate::sim_client::join_state::JoinState;
use crate::sim_client::sent_inputs::SentInputs;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::signer::Signer;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod bot_script;
pub(crate) mod join_state;
pub(crate) mod sent_inputs;
pub(crate) mod server_pin;
pub(crate) mod signer;

/// A client never holds the segment seed: it predicts movement, never a random outcome.
const PREDICTION_SEED: SegmentSeed = SegmentSeed::new([0; 32]);

/// Plays a session on a Lightyear client: answers the server's offer with a delegation of a
/// session key, sends the player's orders as chained inputs, signed once per message with the
/// session key, and runs the sim in every fixed tick, rollbacks included, with the player's own
/// inputs, on the units the client predicts.
#[derive(Debug)]
pub struct SimClient {
    /// The player's Nostr identity, which signs the delegation.
    pub main_key: Keypair,
    /// This session's key, which the delegation lets sign the player's inputs.
    pub session_key: Keypair,
    pub server: ServerPin,
    /// The packages of the mode it plays, which must run at the listing's rate.
    pub packages: Arc<ModePackages>,
    /// Unix seconds: when the delegation is made, and so when it expires.
    pub clock: fn() -> u64,
    /// Fills a seed contribution or BIP-340's auxiliary randomness with random bytes.
    pub entropy: fn(&mut [u8; 32]),
}

/// Orders the player gave, sent in the next fixed tick.
#[derive(Resource, Debug, Default)]
pub struct PendingOrders(Vec<Order>);

impl PendingOrders {
    pub fn push(&mut self, order: Order) {
        self.0.push(order);
    }
}

impl Plugin for SimClient {
    fn build(&self, app: &mut App) {
        let world = app.world_mut();
        let rate = TickRate::new(self.server.tick_hz);
        SimUpdate::prepare(world, PREDICTION_SEED, rate);
        let mut schedule = SimUpdate::schedule();
        // A client hashes no state, so the registry the capabilities fill is not kept.
        let mut state = StateRegistry::new();
        let packages = &self.packages;
        // A client runs no scripts: it predicts only its own player's units.
        packages
            .manifest()
            .capabilities
            .install(world, &mut schedule, &mut state, None);
        packages
            .books(rate, &packages.script_book())
            .install_prediction(world, packages.walkers());
        world.add_schedule(schedule);
        mark_unpredicted(world);
        world.insert_resource(Signer::new(self.session_key, self.entropy));
        world.insert_resource(JoinState::new(
            self.main_key,
            self.server,
            SessionRules::of(packages),
            self.clock,
        ));
        world.init_resource::<SentInputs>();
        // Prediction covers all the latency, with no input delay: an input goes out stamped with
        // the tick the client predicts it in, which Lightyear keeps ahead of the server's by the
        // round trip; with input delay it would keep that tick nearer, and the input would land
        // late.
        app.insert_resource(InputTimelineConfig::new(
            SyncConfig::default(),
            InputDelayConfig::no_input_delay(),
        ));
        app.init_resource::<PendingOrders>();
        app.add_systems(
            Update,
            (
                answer_offer,
                receive_match_start,
                receive_relations,
                receive_match_end,
                report_deaths,
            )
                .chain(),
        );
        app.add_systems(
            FixedUpdate,
            (send_orders.run_if(not(is_in_rollback)), run_predicted_tick)
                .chain()
                .run_if(playing),
        );
    }
}

/// Answers the first offer, unless its terms do not fit: then the client ends its link, as it
/// cannot play the session.
fn answer_offer(
    mut receivers: Query<'_, '_, &mut MessageReceiver<Offer>, With<Client>>,
    mut sender: Single<'_, '_, (Entity, &mut MessageSender<Join>), With<Client>>,
    signer: Res<'_, Signer>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    let (client, ref mut sender) = *sender;
    for mut receiver in &mut receivers {
        for offer in receiver.receive() {
            let session = offer.terms.session_id();
            match state.answer(&offer, &signer) {
                None => {}
                Some(Ok(join)) => {
                    sender.send::<JoinChannel>(join);
                    info!(%session, "joined the offered session");
                }
                Some(Err(mismatch)) => {
                    warn!(%session, %mismatch, "refused the offered session, which ends the link");
                    commands.trigger(Disconnect { entity: client });
                }
            }
        }
    }
}

fn receive_match_start(
    mut receivers: Query<'_, '_, &mut MessageReceiver<MatchStart>, With<Client>>,
    mut state: ResMut<'_, JoinState>,
) {
    for mut receiver in &mut receivers {
        for start in receiver.receive() {
            if state.start(start) {
                MatchStarted {
                    slot: start.slot,
                    start_tick: start.start_tick,
                }
                .log();
            }
        }
    }
}

/// The player plays a match, so this fixed tick runs a sim tick.
fn playing(state: Res<'_, JoinState>) -> bool {
    state.clock().is_some()
}

/// Takes the teams' relations the server sends into the client's world, where its units' targets
/// and filters read them.
fn receive_relations(
    mut receivers: Query<'_, '_, &mut MessageReceiver<Relations>, With<Client>>,
    mut commands: Commands<'_, '_>,
) {
    for mut receiver in &mut receivers {
        for relations in receiver.receive() {
            commands.insert_resource(relations);
        }
    }
}

/// Takes the end of the match into the client's world, which then predicts nothing more.
fn receive_match_end(
    mut receivers: Query<'_, '_, &mut MessageReceiver<MatchEnd>, With<Client>>,
    mut commands: Commands<'_, '_>,
) {
    for mut receiver in &mut receivers {
        for end in receiver.receive() {
            info!(tick = end.tick().get(), result = ?end.result(), "the match ended");
            commands.insert_resource(end);
        }
    }
}

/// The units whose death the server's state just brought, and whether each is the client's own.
type Died<'w, 's> =
    Query<'w, 's, (&'static StableId, Has<Predicted>), (Added<Dead>, Allow<Unpredicted>)>;

/// Logs each death the server's state brings, at the client's own tick, which runs ahead of the
/// server's.
fn report_deaths(timeline: Res<'_, LocalTimeline>, state: Res<'_, JoinState>, died: Died<'_, '_>) {
    let tick = state
        .clock()
        .and_then(|clock| clock.sim_tick(timeline.tick()));
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
/// stamps the pending orders with that tick, up to the session's max inputs per tick, chains each
/// and keeps it, and sends them in one message signed over the chain head after the last. The
/// orders past the max wait for the next tick, so the log never refuses the message; an order
/// whose payload passes the session's max length can never be sent, and is dropped. The inputs
/// older than the deepest rollback Lightyear takes are dropped first.
#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system takes each resource it reads"
)]
fn send_orders(
    timeline: Res<'_, LocalTimeline>,
    prediction: Res<'_, PredictionManager>,
    timeline_config: Res<'_, InputTimelineConfig>,
    bot: Option<ResMut<'_, BotScript>>,
    avatar: OwnAvatar<'_, '_>,
    signer: Res<'_, Signer>,
    mut pending: ResMut<'_, PendingOrders>,
    mut state: ResMut<'_, JoinState>,
    mut sent: ResMut<'_, SentInputs>,
    mut sender: Single<'_, '_, &mut MessageSender<InputMessage>, With<Client>>,
) {
    let Some(playing) = state.playing_mut() else {
        return;
    };
    let Some(stamp) = playing.clock.sim_tick(timeline.tick()) else {
        return;
    };
    let reach = prediction
        .rollback_policy
        .effective_max_rollback_ticks(&timeline_config);
    sent.prune(Tick::new(stamp.get().saturating_sub(u64::from(reach))));
    if let (Some(mut bot), Ok(&unit)) = (bot, avatar.single()) {
        for scripted in bot.due(stamp) {
            pending.push(Order {
                unit,
                action: scripted.action,
            });
        }
    }
    let session = playing.session;
    let first = sent.len();
    let stamped = pending.0.len().min(session.max_inputs as usize);
    for order in pending.0.drain(..stamped) {
        if !sent.push(stamp, &order, session.max_payload_len) {
            warn!(
                ?order,
                "dropped an order whose payload passes the session's max length"
            );
        }
    }
    let payloads = sent.since(first);
    if payloads.len() == 0 {
        return;
    }
    OrdersSent {
        stamp,
        orders: payloads.len(),
    }
    .log();
    for payload in payloads.clone() {
        playing.chain.extend(stamp, payload);
    }
    let slot = playing.chain.slot();
    let inputs = payloads.map(|payload| PlayerInput {
        slot,
        stamp,
        payload,
    });
    let signature = signer.sign(&playing.chain, session.id);
    sender.send::<InputChannel>(InputMessage::new(inputs, signature));
}

/// Runs the sim tick of the current Lightyear tick with the player's inputs stamped for it; in a
/// rollback, Lightyear has wound its timeline back, so the sim follows.
fn run_predicted_tick(world: &mut World) {
    let tick = world.resource::<LocalTimeline>().tick();
    let JoinState::Playing(playing) = world.resource::<JoinState>() else {
        return;
    };
    let slot = playing.chain.slot();
    let Some(sim_tick) = playing.clock.sim_tick(tick) else {
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

/// Marks each replicated entity `Unpredicted` until it is predicted, in whatever order the two
/// markers arrive.
fn mark_unpredicted(world: &mut World) {
    Unpredicted::register(world);
    world.add_observer(
        |added: On<'_, '_, Add, Replicated>,
         predicted: Query<'_, '_, (), With<Predicted>>,
         mut commands: Commands<'_, '_>| {
            if !predicted.contains(added.entity) {
                commands.entity(added.entity).insert(Unpredicted);
            }
        },
    );
    world.add_observer(
        |added: On<'_, '_, Add, Predicted>, mut commands: Commands<'_, '_>| {
            commands.entity(added.entity).remove::<Unpredicted>();
        },
    );
}
