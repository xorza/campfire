use std::sync::Arc;

use bevy_app::{App, AppExit, First, FixedUpdate, Last, Plugin, PreUpdate, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Add, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Added, Allow, Has, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::{
    not, on_message, resource_exists, resource_exists_and_changed,
};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy_ecs::world::{Mut, World};
use bevy_replicon::prelude::Remote;
use campfire_bots::{BotOrder, BotScript, BotUnits, BotView, OrderScratch, ScriptedInput};
use campfire_capabilities::{MatchKind, MatchSetup, Order, Release};
use campfire_common::ErrorReport;
use campfire_common::{SegmentSeed, Tick};
use campfire_declare::Role;
use campfire_log::LogEvent;
use campfire_mode::MatchEnd;
use campfire_net_connection::{Client, Connect, Disconnect};
use campfire_net_endpoint::LinkBinding;
use campfire_net_link::{Link, Unlinked};
use campfire_net_messages::{MessageReceiver, MessageSender};
use campfire_net_payload::UnlinkReason;
use campfire_net_prediction::{PredictionSetup, RollbackSystems};
use campfire_net_replication::{Predicted, ReplicationReceiver};
use campfire_net_stack::ClientPlugins;
use campfire_net_sync::{
    InputDelaySetup, InputTimelineSetup, RemoteTimeline, SyncSetup, SyncedLocalTimeline,
};
use campfire_net_timeline::{Clock, LocalTimeline, Rollback};
use campfire_package::ModePackages;
use campfire_protocol::{SignedReceipt, Signer, Verifier};
use campfire_runner::{SessionRules, TermsError};
use campfire_sim::{
    MatchRate, PredictionStarted, SimTick, SimUpdate, StableId, StateRewritten, TickInput,
    TickInputs, Unpredicted,
};
use campfire_units::{Avatar, Dead, Fallen, Owner, UnitsWorld};
use campfire_values::{Team, TeamSet};
use campfire_vision::ByHolding;
use tracing::info;

use crate::close_code::CloseCode;
use crate::events::bot_order_unsent::BotOrderUnsent;
use crate::events::client_frame::ClientFrame;
use crate::events::input_dropped::InputDropped;
use crate::events::inputs_discarded::InputsDiscarded;
use crate::events::link_lost::{LinkLossCause, LinkLost};
use crate::events::match_ended::MatchEnded;
use crate::events::match_started::MatchStarted;
use crate::events::order_dropped::OrderDropped;
use crate::events::orders_sent::OrdersSent;
use crate::events::receipt_refused::ReceiptRefused;
use crate::events::session_refused::SessionRefused;
use crate::events::unit_died::UnitDied;
use crate::faults::Faults;
use crate::hosting::Hosting;
use crate::net_protocol::dismissed::Dismissed;
use crate::net_protocol::input_ack::InputAck;
use crate::net_protocol::input_message::InputMessage;
use crate::net_protocol::join::{Join, JoinRole};
use crate::net_protocol::leave_match::LeaveMatch;
use crate::net_protocol::match_start::MatchStart;
use crate::net_protocol::offer::Offer;
use crate::net_protocol::save_command::SaveCommand;
use crate::net_protocol::watch_start::WatchStart;
use crate::net_protocol::watch_teams::WatchTeams;
use crate::net_protocol::{InputChannel, JoinChannel, NetProtocol};
use crate::os::Os;
use crate::sim_client::client_dir::ClientDir;
use crate::sim_client::join_state::{JoinState, Joining, LinkLoss, Retry, Started};
use crate::sim_client::player_signer::PlayerSigner;
use crate::sim_client::receipt_writer::ReceiptWriter;
use crate::sim_client::sent_inputs::SentInputs;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::session_signer::SessionSigner;

pub(crate) mod chain_history;
pub(crate) mod client_dir;
pub(crate) mod client_layout;
pub(crate) mod join_state;
pub(crate) mod player_signer;
pub(crate) mod receipt_writer;
pub(crate) mod sent_inputs;
pub(crate) mod server_pin;
pub(crate) mod session_signer;
pub(crate) mod sim_client_world;

/// A client never holds the segment seed: it predicts movement, never a random outcome.
const PREDICTION_SEED: SegmentSeed = SegmentSeed::ZERO;

/// Plays a session on a Lightyear client: answers the server's offer with a delegation of a
/// session key, sends the player's orders as chained inputs, signed once per message with the
/// session key, and runs the sim in every fixed tick, rollbacks included, with the player's own
/// inputs, on the units the client predicts. It adds Lightyear's client at the server's tick, the
/// protocol, and the prediction; its app adds its frame loop or clock, and its link's endpoint: a
/// UDP socket's, or the emulator's in the process.
#[derive(Debug)]
pub struct SimClient {
    /// The signer of the player's Nostr identity, which signs the delegation.
    pub main: Arc<Signer>,
    /// The signer of this session's key, which the delegation lets sign the player's inputs.
    pub session: SessionSigner,
    pub server: ServerPin,
    /// Whether the player plays a seat or watches.
    pub role: JoinRole,
    /// Where the server runs.
    pub hosting: Hosting,
    /// The packages of the mode it plays, which must run at the server's rate: `check`.
    pub packages: Arc<ModePackages>,
    /// Its clock, at which the delegation is made, and so expires, and its random bytes.
    pub os: Os,
    /// The client's data directory, held locked, where it writes the newest receipt of its
    /// session; none writes none.
    pub data: Option<Arc<ClientDir>>,
}

/// The player's wish to leave the match: the client tells the server once, and then tries its
/// link no more. The server logs the leave at once, and ends the link.
#[derive(Resource, Debug)]
pub struct LeaveRequest;

/// A spectator's choice of the teams whose views it watches together, in a session whose
/// spectators watch teams' views: the client tells the server as it starts to watch, again after
/// each rejoin, and as the choice changes.
#[derive(Resource, Debug, Clone, Copy)]
pub struct WatchRequest(pub TeamSet);

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
    /// An error when its packages do not run at the rate its server is pinned to, as a listing
    /// may name a rate the mode does not run at. Its caller checks before it adds the plugin.
    pub fn check(&self) -> Result<(), TermsError> {
        SessionRules::of(&self.packages).runs_at(self.server.tick_hz)
    }

    /// Spawns the client's entity, which its link and its connect name.
    pub fn spawn(world: &mut World) -> Entity {
        world
            .spawn((Client, ReplicationReceiver, Link::default()))
            .id()
    }
}

impl Plugin for SimClient {
    fn build(&self, app: &mut App) {
        assert!(
            self.check().is_ok(),
            "a client's packages run at its server's pinned rate, which its caller checks"
        );
        let rate = MatchRate::new(self.server.tick_hz);
        app.add_plugins(ClientPlugins {
            tick_rate: rate.rate(),
        });
        app.add_plugins(NetProtocol {
            capabilities: self.packages.manifest().capabilities,
        });
        app.insert_resource(PredictionSetup::default());
        let world = app.world_mut();
        let packages = &self.packages;
        // A client runs no scripts: it predicts only its own player's units.
        let set = packages.manifest().capabilities;
        let setup = MatchSetup::new(PREDICTION_SEED, rate, Role::Client).set(set);
        Release::build(world, setup, packages.books(rate), MatchKind::Predicting);
        mark_unpredicted(world);
        world.insert_resource(PlayerSigner(self.session.take()));
        world.insert_resource(self.hosting);
        world.insert_resource(self.os.clone());
        world.insert_resource(JoinState::new(Joining {
            main: Arc::clone(&self.main),
            server: self.server,
            role: self.role,
            packages: Arc::clone(packages),
        }));
        world.init_resource::<SentInputs>();
        if let Some(data) = &self.data {
            world.insert_resource(ReceiptWriter::start(Arc::clone(data)));
        }
        // Prediction covers all the latency, with no input delay: an input goes out stamped with
        // the tick the client predicts it in, which Lightyear keeps ahead of the server's present
        // tick by half the round trip and its margins, so the input arrives before the server
        // runs that tick; with input delay it would keep that tick nearer, and the input would
        // land late.
        app.insert_resource(InputTimelineSetup::new(
            SyncSetup::default(),
            InputDelaySetup::no_input_delay(),
        ));
        app.init_resource::<PendingOrders>();
        app.init_resource::<PendingSaves>();
        app.init_resource::<Faults>();
        app.add_observer(lose_link);
        // The systems on the join state run in the order of a link's life, as one frame may bring
        // an offer and the match's start; none reads what another's commands make, so the order
        // needs no sync point. The faults of the frame apply last.
        let link = (
            retry_link,
            send_leave.run_if(resource_exists::<LeaveRequest>),
            send_saves,
            send_watch_request.run_if(resource_exists_and_changed::<WatchRequest>),
            answer_offer,
            receive_dismissed,
            receive_match_start,
            receive_watch_start,
            receive_receipt,
        )
            .chain_ignore_deferred();
        app.add_systems(
            Update,
            (
                (link, receive_input_acks, report_end, report_deaths),
                ReceiptWriter::report_failure,
                Faults::watch,
            )
                .chain(),
        );
        app.add_systems(
            Last,
            ReceiptWriter::close_at_exit.run_if(on_message::<AppExit>),
        );
        app.add_systems(First, log_frame);
        app.add_systems(
            PreUpdate,
            rewrite_after_rollback
                .after(RollbackSystems::Prepare)
                .before(RollbackSystems::Rollback)
                .run_if(Rollback::is_active),
        );
        app.add_systems(
            FixedUpdate,
            (
                send_orders.run_if(not(Rollback::is_active)),
                run_predicted_tick,
            )
                .chain()
                .run_if(is_playing),
        );
    }
}

/// The client's entity as it answers an offer: its sender of joins, and its link's channel
/// binding, which the answer signs.
type Answering = (
    Entity,
    &'static mut MessageSender<Join>,
    Option<&'static LinkBinding>,
);

/// Answers the first offer, unless its terms do not fit: then the client ends its link, as it
/// cannot play the session.
fn answer_offer(
    mut receivers: Query<'_, '_, &mut MessageReceiver<Offer>, With<Client>>,
    mut sender: Single<'_, '_, Answering, With<Client>>,
    mut signer: ResMut<'_, PlayerSigner>,
    os: Res<'_, Os>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    let (client, ref mut sender, binding) = *sender;
    for mut receiver in &mut receivers {
        for offer in receiver.receive() {
            let session = offer.terms.session_id();
            let LinkBinding(binding) =
                binding.expect("a client that receives an offer holds its link's binding");
            match state.answer(&offer, binding, &mut signer.0, os.clock.unix_now()) {
                None => {}
                Some(Ok(join)) => {
                    let visibility = state
                        .visibility()
                        .expect("a client that answered holds its session's visibility");
                    commands.insert_resource(visibility.out());
                    sender.send::<JoinChannel>(join);
                    info!(%session, "joined the offered session");
                }
                Some(Err(error)) => {
                    SessionRefused {
                        session,
                        mismatch: ErrorReport::of(&error).to_string(),
                    }
                    .log();
                    commands.trigger(Disconnect {
                        entity: client,
                        reason: CloseCode::SessionRefused.app(),
                    });
                }
            }
        }
    }
}

/// Plays the match the server's start names, for a client that answered; see
/// `JoinState::start`. A client whose chain the server rewrote ends its link.
fn receive_match_start(
    mut receivers: Query<'_, '_, (Entity, &mut MessageReceiver<MatchStart>), With<Client>>,
    hosting: Res<'_, Hosting>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    for (client, mut receiver) in &mut receivers {
        for start in receiver.receive() {
            match state.start(start, *hosting) {
                Started::No => {}
                Started::Playing { discarded } => {
                    MatchStarted {
                        seat: start.seat,
                        start_tick: start.start_tick,
                    }
                    .log();
                    if discarded > 0 {
                        InputsDiscarded {
                            seat: start.seat,
                            count: discarded,
                        }
                        .log();
                    }
                }
                Started::Rewritten => {
                    LinkLost {
                        cause: LinkLossCause::ClientClosed(CloseCode::ChainRewritten),
                    }
                    .log();
                    commands.trigger(Disconnect {
                        entity: client,
                        reason: CloseCode::ChainRewritten.app(),
                    });
                }
            }
        }
    }
}

/// Notes a link that failed: a client that knows the server's times drops the inputs it sent and
/// the orders it holds, and tries again; one that does not stops, and logs the cause its link's end
/// gives. Lightyear despawns the units the link replicated.
fn lose_link(
    unlinked: On<'_, '_, Insert<Unlinked>>,
    clients: Query<'_, '_, &Unlinked, With<Client>>,
    clock: Res<'_, Clock>,
    os: Res<'_, Os>,
    mut state: ResMut<'_, JoinState>,
    mut sent: ResMut<'_, SentInputs>,
    mut pending: ResMut<'_, PendingOrders>,
) {
    let Ok(&Unlinked { reason }) = clients.get(unlinked.entity) else {
        return;
    };
    if matches!(reason, UnlinkReason::Initial) {
        return;
    }
    let cause = LinkLossCause::of(reason);
    match state.lose(clock.now().elapsed(), os.entropy.random()) {
        LinkLoss::Rejoining => {
            info!(?cause, "the link to the server failed; trying again");
            sent.clear();
            pending.clear();
        }
        LinkLoss::Stopped(_) => LinkLost { cause }.log(),
        LinkLoss::Nothing => {}
    }
}

/// Keeps each receipt the client accepts, and hands it to the data directory's writer, when the
/// client keeps one.
fn receive_receipt(
    mut receivers: Query<'_, '_, &mut MessageReceiver<SignedReceipt>, With<Client>>,
    mut state: ResMut<'_, JoinState>,
    writer: Option<Res<'_, ReceiptWriter>>,
    mut verifier: Local<'_, Verifier>,
) {
    for mut receiver in &mut receivers {
        for receipt in receiver.receive() {
            if let Err(refusal) = state.take_receipt(&receipt, &mut verifier) {
                ReceiptRefused { refusal }.log();
                continue;
            }
            if let Some(writer) = &writer {
                writer.give(receipt);
            }
        }
    }
}

/// Stops a client the server dismissed, and ends its link with the dismissal's code, which tells
/// the server that the word arrived.
fn receive_dismissed(
    mut receivers: Query<'_, '_, (Entity, &mut MessageReceiver<Dismissed>), With<Client>>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    for (client, mut receiver) in &mut receivers {
        let Some(dismissed) = receiver.receive().next() else {
            continue;
        };
        state.dismiss(dismissed);
        let code = dismissed.code();
        LinkLost {
            cause: LinkLossCause::ClientClosed(code),
        }
        .log();
        commands.trigger(Disconnect {
            entity: client,
            reason: code.app(),
        });
    }
}

/// Tells the server the teams a spectator watches, as its choice changes while it watches.
fn send_watch_request(
    mut sender: Single<'_, '_, &mut MessageSender<WatchTeams>, With<Client>>,
    state: Res<'_, JoinState>,
    request: Res<'_, WatchRequest>,
) {
    if state.watching_clock().is_some() {
        sender.send::<JoinChannel>(WatchTeams(request.0));
    }
}

/// Watches the match the server's start names, for a client that answered to watch, and tells
/// the server the teams it chose, as each join starts it on every team's; see
/// `JoinState::watch`.
fn receive_watch_start(
    mut links: Query<
        '_,
        '_,
        (
            &mut MessageReceiver<WatchStart>,
            &mut MessageSender<WatchTeams>,
        ),
        With<Client>,
    >,
    mut state: ResMut<'_, JoinState>,
    request: Option<Res<'_, WatchRequest>>,
) {
    for (mut receiver, mut sender) in &mut links {
        for start in receiver.receive() {
            if !state.watch(start) {
                continue;
            }
            info!(first = start.first.get(), "watching the match");
            if let Some(request) = &request {
                sender.send::<JoinChannel>(WatchTeams(request.0));
            }
        }
    }
}

/// Sends the player's save commands, while the client plays.
fn send_saves(
    mut sender: Single<'_, '_, &mut MessageSender<SaveCommand>, With<Client>>,
    state: Res<'_, JoinState>,
    mut pending: ResMut<'_, PendingSaves>,
) {
    if state.playing_clock().is_none() {
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
    if state.is_left() {
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
    clock: Res<'_, Clock>,
    os: Res<'_, Os>,
    mut state: ResMut<'_, JoinState>,
    mut commands: Commands<'_, '_>,
) {
    let (entity, unlinked) = *client;
    if !state.is_rejoining() || !unlinked {
        return;
    }
    match state.retry(clock.now().elapsed(), os.entropy.random()) {
        Retry::Wait => {}
        Retry::Connect => {
            info!("connecting to the server again");
            commands.trigger(Connect { entity });
        }
        Retry::GiveUp => LinkLost {
            cause: LinkLossCause::RetriesEnded,
        }
        .log(),
    }
}

/// Logs the start of each frame in which the client plays, with the sim tick it predicts next.
fn log_frame(state: Res<'_, JoinState>, timeline: Option<SyncedLocalTimeline<'_, '_>>) {
    if let Some(tick) = state.sim_tick(timeline.as_ref()) {
        ClientFrame { tick }.log();
    }
}

/// The client runs a sim tick in this fixed tick; see `JoinState::sim_tick`. A match start's
/// clock names the server's Lightyear tick, which a server started again counts from 0, while the
/// client's timeline counts the old link's until its first sync on the new one shifts it.
fn is_playing(state: Res<'_, JoinState>, timeline: Option<SyncedLocalTimeline<'_, '_>>) -> bool {
    state.sim_tick(timeline.as_ref()).is_some()
}

/// Takes the server's word on where each of the player's inputs takes effect, so a rollback
/// replays it there.
fn receive_input_acks(
    mut receivers: Query<'_, '_, &mut MessageReceiver<InputAck>, With<Client>>,
    mut sent: ResMut<'_, SentInputs>,
) {
    for mut receiver in &mut receivers {
        for ack in receiver.receive() {
            sent.acknowledge(ack.first, &ack.applied);
        }
    }
}

/// Logs the end of the match, which reaches the client on the match's entity: its sim then
/// predicts nothing more.
fn report_end(ended: Query<'_, '_, &MatchEnd, (Added<MatchEnd>, Allow<Unpredicted>)>) {
    for &end in &ended {
        MatchEnded::of(end).log();
    }
}

/// The units whose death the server's state just brought, with their team, their owner, and
/// whether each is its owner's avatar.
type Died<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        Option<&'static Team>,
        Option<&'static Owner>,
        Has<Avatar>,
    ),
    (Added<Dead>, Allow<Unpredicted>),
>;

/// Logs each death the server's state brings, at the client's own tick, which runs ahead of the
/// server's, once its timeline synced.
fn report_deaths(
    timeline: Option<SyncedLocalTimeline<'_, '_>>,
    state: Res<'_, JoinState>,
    died: Died<'_, '_>,
) {
    let tick = state.sim_tick(timeline.as_ref());
    for (&id, team, owner, avatar) in &died {
        let fallen = Fallen {
            id,
            team: team.copied(),
            owner: owner.map(|owner| owner.seat()),
            avatar,
        };
        UnitDied::learned(tick, fallen).log();
    }
}

/// Adds the bot script's mode inputs due by the tick about to run, and its orders, each for the
/// player's units it names: one for the player's own avatar once the client predicts it, as a link
/// that came back replicates it a little later, so an order due before waits with those after it,
/// and goes out late; one whose units the player lacks is dropped. Then stamps the pending orders
/// with that tick, up to the session's max inputs per tick, chains each and keeps it, and sends
/// them in one message signed over the chain head after the last. The orders past the max wait for
/// the next tick, so the log never refuses the message; an order whose payload passes the session's
/// max length can never be sent, and is dropped. The inputs older than the deepest rollback
/// Lightyear takes are dropped first.
#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system takes each resource it reads"
)]
fn send_orders(
    timeline: Res<'_, LocalTimeline>,
    prediction: Res<'_, PredictionSetup>,
    timeline_config: Res<'_, InputTimelineSetup>,
    bot: Option<ResMut<'_, BotScript>>,
    view: BotView<'_, '_, ByHolding>,
    mut scratch: Local<'_, OrderScratch>,
    mut orders: Local<'_, Vec<Order>>,
    signer: Res<'_, PlayerSigner>,
    mut pending: ResMut<'_, PendingOrders>,
    mut state: ResMut<'_, JoinState>,
    mut sent: ResMut<'_, SentInputs>,
    mut sender: Single<'_, '_, (&mut MessageSender<InputMessage>, &RemoteTimeline), With<Client>>,
) {
    let (ref mut sender, remote) = *sender;
    let Some(playing) = state.playing_mut() else {
        return;
    };
    let Some(now) = playing.clock.sim_tick(timeline.tick()) else {
        return;
    };
    let stamp = playing.stamp(now);
    // An input stamped past the max input lead beyond the server's next tick is logged as early
    // and never applies, as after a server stall that let the client run ahead: the orders wait
    // for the server instead. The last tick the server sent is at most its next, so an input
    // within the lead of it is within the lead of the next as it arrives.
    let server = remote
        .last_received_tick()
        .and_then(|tick| playing.clock.sim_tick(tick));
    let lead = playing.member.session.inputs.max_lead;
    if server.is_some_and(|server| stamp.since(server).is_some_and(|ahead| ahead > lead)) {
        return;
    }
    let reach = prediction.max_rollback_ticks(&timeline_config);
    sent.prune(Tick::new(now.get().saturating_sub(u64::from(reach))));
    if let Some(mut bot) = bot {
        for input in bot.due_inputs(stamp) {
            pending.push_input(input.clone());
        }
        let seat = playing.chain.seat();
        let avatar = view.avatar(seat).is_some();
        let ready = |order: &BotOrder| avatar || order.units != BotUnits::Avatar;
        for scripted in bot.due_orders_while(stamp, ready) {
            orders.clear();
            let unsent = scripted.orders(seat, &view, &mut scratch, &mut orders);
            for order in orders.drain(..) {
                pending.push(order);
            }
            if let Some(cause) = unsent {
                BotOrderUnsent {
                    seat,
                    cause,
                    action: scripted.named.clone(),
                }
                .log();
            }
        }
    }
    let session = playing.member.session;
    let first = sent.len();
    let stamped = pending
        .0
        .len()
        .min(usize::try_from(session.inputs.max_per_tick).expect("a u32 fits usize"));
    for pending in pending.0.drain(..stamped) {
        let seq =
            playing.chain.next_seq() + u64::try_from(sent.len() - first).expect("a count fits");
        match pending {
            Pending::Order(order) => {
                let kept = sent.push(seq, stamp, session.inputs.max_payload_len, |body, out| {
                    order.write_payload(body, out);
                });
                if !kept {
                    let units = order.units.get();
                    OrderDropped {
                        unit: units[0],
                        units: units.len(),
                        action: order.action,
                    }
                    .log();
                }
            }
            Pending::Input(input) => {
                let kept = sent.push(seq, stamp, session.inputs.max_payload_len, |body, out| {
                    input.write_payload(body, out);
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
        seat: playing.chain.seat(),
        seq: playing.chain.next_seq(),
        stamp,
        orders: payloads.len(),
    }
    .log();
    for payload in payloads.clone() {
        playing.extend(stamp, payload);
    }
    let inputs = payloads.map(|payload| (stamp, payload));
    let signature = signer.0.sign(&playing.chain.signed_head(session.id));
    sender.send::<InputChannel>(InputMessage::new(inputs, signature));
}

/// Runs the sim tick of the current Lightyear tick with the player's inputs stamped for it; in a
/// rollback, Lightyear has wound its timeline back, so the sim follows. The match's state, which
/// every tick reads, reaches the client on the match's entity: until it holds it, the client
/// predicts nothing, as it holds no unit either.
fn run_predicted_tick(world: &mut World) {
    let tick = world.resource::<LocalTimeline>().tick();
    let Some(playing) = world.resource::<JoinState>().playing() else {
        return;
    };
    if world.match_entity().is_none() {
        return;
    }
    let seat = playing.chain.seat();
    let Some(sim_tick) = playing.clock.sim_tick(tick) else {
        return;
    };
    *world.resource_mut::<SimTick>() = SimTick::new(sim_tick);
    world.resource_scope(|world, sent: Mut<'_, SentInputs>| {
        let mut inputs = world.resource_mut::<TickInputs>();
        for payload in sent.at(sim_tick) {
            inputs.push(TickInput { seat, payload });
        }
    });
    world.run_schedule(SimUpdate);
}

/// Tells the sim, once the stack restored the parts it predicts and before it runs the ticks
/// again, that its state was written from outside it, so each cache that follows the state's
/// changes builds again from the restored state (04-capabilities/00-overview.md, One due index for
/// each timed process).
fn rewrite_after_rollback(world: &mut World) {
    world.trigger(StateRewritten);
}

/// Marks each replicated entity `Unpredicted` while it is not predicted, in whatever order the
/// two markers arrive, and again as the server stops its prediction here, as its owner changes.
/// The client tells the sim of each unit it starts to predict, as it comes predicted or as its
/// prediction begins, after the batch that brought it, so the sim's caches take its parts.
fn mark_unpredicted(world: &mut World) {
    world.add_observer(
        |added: On<'_, '_, Add<Remote>>,
         predicted: Query<'_, '_, (), With<Predicted>>,
         mut commands: Commands<'_, '_>| {
            if !predicted.contains(added.entity) {
                commands.entity(added.entity).insert(Unpredicted);
            }
        },
    );
    world.add_observer(
        |added: On<'_, '_, Add<Predicted>>, mut commands: Commands<'_, '_>| {
            commands.entity(added.entity).remove::<Unpredicted>();
            commands.trigger(PredictionStarted {
                entity: added.entity,
            });
        },
    );
    world.add_observer(
        |removed: On<'_, '_, Remove<Predicted>>,
         replicated: Query<'_, '_, (), With<Remote>>,
         mut commands: Commands<'_, '_>| {
            if replicated.contains(removed.entity) {
                commands.entity(removed.entity).try_insert(Unpredicted);
            }
        },
    );
}
