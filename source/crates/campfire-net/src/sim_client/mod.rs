use std::sync::Arc;

use bevy_app::{App, FixedUpdate, Plugin, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Add, Insert};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Added, Allow, Has, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::{not, resource_exists};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy_ecs::world::{Mut, World};
use bevy_time::{Real, Time};
use campfire_capabilities::{Dead, MatchEnd, Order, Owner, Relations};
use campfire_common::{SegmentSeed, Tick};
use campfire_log::{ErrorReport, LogEvent};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, VerifyOnly};
use campfire_protocol::{PlayerInput, SignedReceipt};
use campfire_runner::SessionRules;
use campfire_sim::{
    SimTick, SimUpdate, StableId, StateRegistry, TickInput, TickInputs, TickRate, Unpredicted,
};
use lightyear::prelude::client::{ClientPlugins, InputDelayConfig, InputTimelineConfig, RawClient};
use lightyear::prelude::{
    Client, Connect, Disconnect, LocalTimeline, MessageReceiver, MessageSender, Predicted,
    PredictionManager, Replicated, ReplicationReceiver, SyncConfig, UnlinkReason, Unlinked,
    is_in_rollback,
};
use tracing::{debug, info};

use crate::events::input_dropped::InputDropped;
use crate::events::inputs_discarded::InputsDiscarded;
use crate::events::link_lost::LinkLost;
use crate::events::match_started::MatchStarted;
use crate::events::order_dropped::OrderDropped;
use crate::events::orders_sent::OrdersSent;
use crate::events::receipt_refused::ReceiptRefused;
use crate::events::session_refused::SessionRefused;
use crate::faults::Faults;
use crate::input_message::InputMessage;
use crate::join::Join;
use crate::leave_match::LeaveMatch;
use crate::match_start::MatchStart;
use crate::net_protocol::{InputChannel, JoinChannel, NetProtocol};
use crate::offer::Offer;
use crate::order_script::ScriptedInput;
use crate::save_command::SaveCommand;
use crate::sim_client::bot_script::BotScript;
use crate::sim_client::client_dir::ClientDir;
use crate::sim_client::join_state::{JoinState, LinkLoss, Retry, Started};
use crate::sim_client::receipt_writer::ReceiptWriter;
use crate::sim_client::sent_inputs::SentInputs;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::signer::Signer;
use crate::superseded::Superseded;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod bot_script;
pub(crate) mod chain_history;
pub(crate) mod client_dir;
pub(crate) mod join_state;
pub(crate) mod receipt_writer;
pub(crate) mod sent_inputs;
pub(crate) mod server_pin;
pub(crate) mod signer;

/// A client never holds the segment seed: it predicts movement, never a random outcome.
const PREDICTION_SEED: SegmentSeed = SegmentSeed::new([0; 32]);

/// Plays a session on a Lightyear client: answers the server's offer with a delegation of a
/// session key, sends the player's orders as chained inputs, signed once per message with the
/// session key, and runs the sim in every fixed tick, rollbacks included, with the player's own
/// inputs, on the units the client predicts. It adds Lightyear's client at the server's tick, the
/// protocol, and the prediction; its app adds its frame loop or clock, and its link.
#[derive(Debug)]
pub struct SimClient {
    /// The player's Nostr identity, which signs the delegation.
    pub main_key: Keypair,
    /// This session's key, which the delegation lets sign the player's inputs.
    pub session_key: Keypair,
    pub server: ServerPin,
    /// Whether the server runs on a thread of the client's process, the only one that loads a
    /// save.
    pub local: bool,
    /// The packages of the mode it plays, which must run at the listing's rate.
    pub packages: Arc<ModePackages>,
    /// Unix seconds: when the delegation is made, and so when it expires.
    pub clock: fn() -> u64,
    /// Fills a seed contribution or BIP-340's auxiliary randomness with random bytes.
    pub entropy: fn(&mut [u8; 32]),
    /// The client's data directory, held locked, where it writes the newest receipt of its
    /// session; none writes none.
    pub data: Option<Arc<ClientDir>>,
}

/// The context that checks receipts' signatures.
#[derive(Debug)]
struct Verifier(Secp256k1<VerifyOnly>);

impl Default for Verifier {
    fn default() -> Verifier {
        Verifier(Secp256k1::verification_only())
    }
}

/// The player's wish to leave the match: the client tells the server once, and then tries its
/// link no more. The server logs the leave at once, and ends the link.
#[derive(Resource, Debug)]
pub struct LeaveRequest;

/// The player's save commands, which a local server takes, sent while the client plays.
#[derive(Resource, Debug, Default)]
pub struct PendingSaves(Vec<SaveCommand>);

impl PendingSaves {
    pub fn push(&mut self, command: SaveCommand) {
        self.0.push(command);
    }
}

/// Orders the player gave, and mode inputs a bot script gives, sent in the next fixed tick.
#[derive(Resource, Debug, Default)]
pub struct PendingOrders(Vec<Pending>);

/// One pending input.
#[derive(Debug)]
enum Pending {
    Order(Order),
    Input(ScriptedInput),
}

impl PendingOrders {
    pub fn push(&mut self, order: Order) {
        self.0.push(Pending::Order(order));
    }

    fn push_input(&mut self, input: ScriptedInput) {
        self.0.push(Pending::Input(input));
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

impl SimClient {
    /// Spawns the client's entity in `world`, which its link and its connect name.
    pub fn spawn_client(world: &mut World) -> Entity {
        world.spawn((Client, RawClient, ReplicationReceiver)).id()
    }
}

impl Plugin for SimClient {
    fn build(&self, app: &mut App) {
        let rate = TickRate::new(self.server.tick_hz);
        app.add_plugins(ClientPlugins {
            tick_duration: rate.length(),
        });
        app.add_plugins(NetProtocol);
        app.insert_resource(PredictionManager::default());
        let world = app.world_mut();
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
            .books(rate)
            .install_prediction(world, packages.walkers());
        world.add_schedule(schedule);
        mark_unpredicted(world);
        world.insert_resource(Signer::new(self.session_key, self.entropy));
        world.insert_resource(JoinState::new(
            self.main_key,
            self.server,
            self.local,
            SessionRules::of(packages),
            self.clock,
        ));
        world.init_resource::<SentInputs>();
        if let Some(data) = &self.data {
            world.insert_resource(ReceiptWriter::start(Arc::clone(data)));
        }
        // Prediction covers all the latency, with no input delay: an input goes out stamped with
        // the tick the client predicts it in, which Lightyear keeps ahead of the server's present
        // tick by half the round trip and its margins, so the input arrives before the server
        // runs that tick; with input delay it would keep that tick nearer, and the input would
        // land late.
        app.insert_resource(InputTimelineConfig::new(
            SyncConfig::default(),
            InputDelayConfig::no_input_delay(),
        ));
        app.init_resource::<PendingOrders>();
        app.init_resource::<PendingSaves>();
        app.init_resource::<Faults>();
        app.add_observer(lose_link);
        app.add_systems(
            Update,
            (
                retry_link,
                send_leave.run_if(resource_exists::<LeaveRequest>),
                send_saves,
                answer_offer,
                receive_superseded,
                receive_match_start,
                receive_receipt,
                Faults::watch,
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
    mut signer: ResMut<'_, Signer>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    let (client, ref mut sender) = *sender;
    for mut receiver in &mut receivers {
        for offer in receiver.receive() {
            let session = offer.terms.session_id();
            match state.answer(&offer, &mut signer) {
                None => {}
                Some(Ok(join)) => {
                    sender.send::<JoinChannel>(join);
                    info!(%session, "joined the offered session");
                }
                Some(Err(error)) => {
                    SessionRefused {
                        session,
                        mismatch: ErrorReport::of(&error).to_string(),
                    }
                    .log();
                    commands.trigger(Disconnect { entity: client });
                }
            }
        }
    }
}

/// Plays the match the server's start names, for a client that answered; see
/// `JoinState::start`. A client whose chain the server rewrote ends its link.
fn receive_match_start(
    mut receivers: Query<'_, '_, (Entity, &mut MessageReceiver<MatchStart>), With<Client>>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    for (client, mut receiver) in &mut receivers {
        for start in receiver.receive() {
            match state.start(start) {
                Started::No => {}
                Started::Playing { discarded } => {
                    MatchStarted {
                        slot: start.slot,
                        start_tick: start.start_tick,
                    }
                    .log();
                    if discarded > 0 {
                        InputsDiscarded {
                            slot: start.slot,
                            count: discarded,
                        }
                        .log();
                    }
                }
                Started::Rewritten => {
                    LinkLost {
                        reason: "the server's copy of the player's chain is not theirs".to_owned(),
                    }
                    .log();
                    commands.trigger(Disconnect { entity: client });
                }
            }
        }
    }
}

/// Notes a link that failed: a client that knows the server's times drops the inputs it sent and
/// the orders it holds, and tries again; one that does not stops. Lightyear despawns the units
/// the link replicated.
fn lose_link(
    unlinked: On<'_, '_, Insert, Unlinked>,
    clients: Query<'_, '_, &Unlinked, With<Client>>,
    time: Res<'_, Time<Real>>,
    signer: Res<'_, Signer>,
    mut state: ResMut<'_, JoinState>,
    mut sent: ResMut<'_, SentInputs>,
    mut pending: ResMut<'_, PendingOrders>,
) {
    let Ok(Unlinked { reason }) = clients.get(unlinked.entity) else {
        return;
    };
    if matches!(reason, UnlinkReason::Initial) {
        return;
    }
    match state.lose(time.elapsed(), signer.random()) {
        LinkLoss::Rejoining => {
            info!(%reason, "the link to the server failed; trying again");
            sent.clear();
            pending.clear();
        }
        LinkLoss::Stopped(_) => LinkLost {
            reason: reason.to_string(),
        }
        .log(),
        LinkLoss::Nothing => {}
    }
}

/// Keeps each receipt the client accepts, and hands it to the data directory's writer, when the
/// client keeps one.
fn receive_receipt(
    mut receivers: Query<'_, '_, &mut MessageReceiver<SignedReceipt>, With<Client>>,
    mut state: ResMut<'_, JoinState>,
    writer: Option<Res<'_, ReceiptWriter>>,
    verifier: Local<'_, Verifier>,
) {
    for mut receiver in &mut receivers {
        for receipt in receiver.receive() {
            if let Err(error) = state.take_receipt(&receipt, &verifier.0) {
                ReceiptRefused {
                    reason: ErrorReport::of(&error).to_string(),
                }
                .log();
                continue;
            }
            if let Some(writer) = &writer {
                writer.give(receipt);
            }
        }
    }
}

/// Stops a client whose slot a newer login of its player took.
fn receive_superseded(
    mut receivers: Query<'_, '_, &mut MessageReceiver<Superseded>, With<Client>>,
    mut state: ResMut<'_, JoinState>,
) {
    for mut receiver in &mut receivers {
        if receiver.receive().count() > 0 {
            state.supersede();
            LinkLost {
                reason: "a newer login of the player took the slot".to_owned(),
            }
            .log();
        }
    }
}

/// Sends the player's save commands, while the client plays.
fn send_saves(
    mut sender: Single<'_, '_, &mut MessageSender<SaveCommand>, With<Client>>,
    state: Res<'_, JoinState>,
    mut pending: ResMut<'_, PendingSaves>,
) {
    if state.clock().is_none() {
        return;
    }
    for command in pending.0.drain(..) {
        sender.send::<JoinChannel>(command);
    }
}

/// Tells the server that the player leaves, once, and stops the client's tries.
fn send_leave(
    mut sender: Single<'_, '_, &mut MessageSender<LeaveMatch>, With<Client>>,
    mut state: ResMut<'_, JoinState>,
) {
    if state.left() {
        return;
    }
    info!("leaving the match");
    sender.send::<JoinChannel>(LeaveMatch);
    state.leave();
}

/// Connects again a client that tries its link again, once its wait passed, while its link is
/// down; stops once the server's grace period and restore window passed.
fn retry_link(
    client: Single<'_, '_, (Entity, Has<Unlinked>), With<Client>>,
    time: Res<'_, Time<Real>>,
    signer: Res<'_, Signer>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    let (entity, unlinked) = *client;
    if !state.rejoining() || !unlinked {
        return;
    }
    match state.retry(time.elapsed(), signer.random()) {
        Retry::Wait => {}
        Retry::Connect => {
            info!("connecting to the server again");
            commands.trigger(Connect { entity });
        }
        Retry::GiveUp => LinkLost {
            reason: "the server's grace period and restore window passed".to_owned(),
        }
        .log(),
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
    if let Some(mut bot) = bot {
        let due = bot.due(stamp);
        for input in due.inputs {
            pending.push_input(input.clone());
        }
        if let Ok(&unit) = avatar.single() {
            for scripted in due.orders {
                pending.push(Order {
                    unit,
                    action: scripted.action,
                });
            }
        }
    }
    let session = playing.member.session;
    let first = sent.len();
    let stamped = pending.0.len().min(session.max_inputs as usize);
    for pending in pending.0.drain(..stamped) {
        match pending {
            Pending::Order(order) => {
                let kept = sent.push(stamp, session.max_payload_len, |body, out| {
                    order.write_payload(body, out);
                });
                if !kept {
                    OrderDropped {
                        unit: order.unit,
                        action: format!("{:?}", order.action),
                    }
                    .log();
                }
            }
            Pending::Input(input) => {
                let kept = sent.push(stamp, session.max_payload_len, |_, out| {
                    input.write_payload(out);
                });
                if !kept {
                    InputDropped { name: input.name }.log();
                }
            }
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
        playing.extend(stamp, payload);
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
    let Some(playing) = world.resource::<JoinState>().playing() else {
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
