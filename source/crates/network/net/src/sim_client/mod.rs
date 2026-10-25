use campfire_values::TagSet;
use campfire_values::UnitTags;
use std::sync::Arc;

use bevy_app::{App, AppExit, First, FixedUpdate, Last, Plugin, PreUpdate, Update};
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Add, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Added, Allow, Has, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::{not, on_message, resource_changed, resource_exists};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut, Single, SystemParam};
use bevy_ecs::world::{EntityWorldMut, Mut, World};
use bevy_replicon::prelude::Remote;
use bevy_time::{Fixed, Time};
use campfire_bots::{BotOrder, BotScript, BotUnits, ClientBotView, OrderScratch, ScriptedInput};
use campfire_capabilities::{MatchKind, MatchSetup, Order, OrderPayloads, Release};
use campfire_common::ErrorReport;
use campfire_common::{SegmentSeed, Tick};
use campfire_declarations::Role;
use campfire_log::LogEvent;
use campfire_mode::MatchEnd;
use campfire_net_connection::{Client, Connect, Disconnect};
use campfire_net_endpoint::LinkBinding;
use campfire_net_interpolation::InterpolationSystems;
use campfire_net_link::{Link, Unlinked};
use campfire_net_messages::{MessageReceiver, MessageSender};
use campfire_net_payload::UnlinkReason;
use campfire_net_prediction::{PredictionSetup, RollbackSystems};
use campfire_net_replication::{
    ConfirmedTickMap, Predicted, ReplicationReceiver, ReplicationSystems,
};
use campfire_net_stack::ClientPlugins;
use campfire_net_sync::{InputTimelineSetup, RemoteTimeline, SyncedLocalTimeline};
use campfire_net_timeline::{Clock, LocalTimeline, Rollback};
use campfire_package::ModePackages;
use campfire_protocol::{SignedReceipt, Signer, Verifier};
use campfire_runner::{SessionRules, TermsError};
use campfire_sim::{
    MatchRate, PredictionStarted, SimTick, SimUpdate, StableId, StateRewritten, TickInput,
    TickInputs, Unpredicted,
};
use campfire_units::{Avatar, Dead, Fallen, Owner, UnitsWorld};
use campfire_values::Team;
use tracing::info;

use crate::close_code::CloseCode;
use crate::events::link_lost::{LinkLossCause, LinkLost};
use crate::net_protocol::{InputChannel, JoinChannel, NetProtocolPlugin};
use crate::sim_client::bot_leave::BotLeave;
use campfire_host::BotOrderUnsent;
use campfire_host::ClientDir;
use campfire_host::ClientFrame;
use campfire_host::Dismissed;
use campfire_host::Hosting;
use campfire_host::InputAck;
use campfire_host::InputMessage;
use campfire_host::InputsDiscarded;
use campfire_host::KnownCommanded;
use campfire_host::LeaveMatch;
use campfire_host::MatchEnded;
use campfire_host::MatchLoaded;
use campfire_host::MatchStart;
use campfire_host::MatchStarted;
use campfire_host::OrdersSent;
use campfire_host::Os;
use campfire_host::PlayerSigner;
use campfire_host::ReceiptRefused;
use campfire_host::ReceiptWriter;
use campfire_host::SaveCommand;
use campfire_host::SentInputs;
use campfire_host::ServerPin;
use campfire_host::SessionOffer;
use campfire_host::SessionRefused;
use campfire_host::SessionSigner;
use campfire_host::UnitDied;
use campfire_host::WatchStart;
use campfire_host::WatchTeams;
use campfire_host::WorkerFaults;
use campfire_host::{Join, JoinRole};
use campfire_host::{JoinState, Joining, LinkLoss, Retry, StartEffect};

pub(crate) mod bot_leave;

/// A client never holds the segment seed: it predicts movement, never a random outcome.
const PREDICTION_SEED: SegmentSeed = SegmentSeed::ZERO;

/// Plays a session on a network stack client: answers the server's offer with a delegation of a
/// session key, sends the player's orders, or those of its bot script, as chained inputs, signed
/// once per message with the session key, and runs the sim in every fixed tick, rollbacks included,
/// with the player's own inputs, on the units the client predicts. It adds the network stack's
/// client at the server's tick, the protocol, and the prediction; its app adds its frame loop or
/// clock, and its link's endpoint: a UDP socket's, or the emulator's in the process.
#[derive(Debug)]
pub struct SimClientPlugin {
    /// The signer of the player's Nostr identity, which signs the delegation.
    pub main: Arc<Signer>,
    /// The signer of this session's key, which the delegation lets sign the player's inputs.
    pub session: SessionSigner,
    pub server: ServerPin,
    /// Whether the player plays a player slot or watches.
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
    /// The order script the client plays for the player, which leaves after the tick the script
    /// ends at when it names one; none for a player's own orders.
    pub bot: Option<BotScript>,
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

/// The inputs the client holds: those it sent, what their acks tell of the units the server
/// commands, and those that wait to be sent, which a client that joins again drops together.
#[derive(SystemParam, Debug)]
struct HeldInputs<'w> {
    sent: ResMut<'w, SentInputs>,
    known: ResMut<'w, KnownCommanded>,
    pending: ResMut<'w, PendingOrders>,
}

impl HeldInputs<'_> {
    fn clear(&mut self) {
        self.sent.clear();
        self.known.clear();
        self.pending.clear();
    }
}

/// One pending input: an order, a mode input, or a part of an order's select past the inputs one
/// tick takes, which waits for the next.
#[derive(Debug)]
enum Pending {
    Order(Order),
    Input(ScriptedInput),
    Part(Vec<u8>),
}

/// What `PendingOrders::stamp` stamps its inputs into: the tick of their stamp, the seq of the
/// first, the most inputs it may stamp, the session's max payload length, the inputs sent, what
/// their acks tell of the units the server commands, and the buffer an order's payloads are
/// written in.
#[derive(Debug)]
struct Stamping<'a> {
    stamp: Tick,
    next_seq: u64,
    room: u32,
    max_len: u32,
    sent: &'a mut SentInputs,
    known: &'a mut KnownCommanded,
    payloads: &'a mut OrderPayloads,
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

    /// Stamps its inputs into `stamping.sent`, in order, up to `stamping.room`:
    /// an order to the units the server is known to command with no select, any other with its
    /// select, split past one payload, whose parts past the tick's inputs wait at its front for
    /// the next (04-capabilities/control.md, A select goes with each order until its sender knows
    /// the server commands those units). Every input fits a payload, as the terms the client
    /// joined hold every input the engine makes (05-protocol-spec.md, A session's terms hold every
    /// input).
    fn stamp(&mut self, stamping: Stamping<'_>) {
        let Stamping {
            stamp,
            next_seq,
            room,
            max_len,
            sent,
            known,
            payloads,
        } = stamping;
        let first = sent.len();
        let seq =
            |sent: &SentInputs| next_seq + u64::try_from(sent.len() - first).expect("a count fits");
        let mut left = usize::try_from(room).expect("a u32 fits usize");
        let mut done = 0;
        let mut carried = Vec::new();
        while left > 0 && done < self.0.len() {
            match &self.0[done] {
                Pending::Order(order) if known.commands(order.units.get()) => {
                    let seq = seq(sent);
                    sent.push(seq, stamp, max_len, |body, out| {
                        order.write_commanded_payload(body, out);
                    });
                    left -= 1;
                }
                Pending::Order(order) => {
                    let fits = order.write_payloads(max_len, payloads);
                    assert!(
                        fits,
                        "an order fits payloads, as the joined terms hold every input"
                    );
                    let parts = payloads.iter().len();
                    for (at, part) in payloads.iter().enumerate() {
                        if at >= left {
                            carried.push(part.to_vec());
                            continue;
                        }
                        let seq = seq(sent);
                        sent.push(seq, stamp, max_len, |_, out| out.extend_from_slice(part));
                        if parts == 1 {
                            known.sent(seq, order.units.get());
                        } else {
                            known.sent_part(seq);
                        }
                    }
                    left -= parts.min(left);
                }
                Pending::Part(part) => {
                    let seq = seq(sent);
                    sent.push(seq, stamp, max_len, |_, out| out.extend_from_slice(part));
                    known.sent_part(seq);
                    left -= 1;
                }
                Pending::Input(input) => {
                    let seq = seq(sent);
                    sent.push(seq, stamp, max_len, |body, out| {
                        input.write_payload(body, out);
                    });
                    left -= 1;
                }
            }
            done += 1;
            if !carried.is_empty() {
                break;
            }
        }
        self.0.drain(..done);
        self.0.splice(0..0, carried.into_iter().map(Pending::Part));
    }
}

impl SimClientPlugin {
    /// An error when its packages do not run at the rate its server is pinned to, as a listing
    /// may name a rate the mode does not run at. Its caller checks before it adds the plugin.
    pub fn check(&self) -> Result<(), TermsError> {
        SessionRules::of(&self.packages).runs_at(self.server.tick_hz)
    }

    /// Spawns the client's entity, which its link and its connect name.
    pub fn spawn(world: &mut World) -> Entity {
        let mut client = world.spawn((Client, Link::default()));
        Self::insert_parts(&mut client);
        client.id()
    }

    /// Inserts on `client`, a client's entity of the stack's `Client` and `Link`, the parts the
    /// sim client reads of it: the replication's receiver.
    pub fn insert_parts(client: &mut EntityWorldMut<'_>) {
        client.insert(ReplicationReceiver);
    }
}

impl Plugin for SimClientPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            self.check().is_ok(),
            "a client's packages run at its server's pinned rate, which its caller checks"
        );
        let rate = MatchRate::new(self.server.tick_hz);
        app.add_plugins(ClientPlugins {
            tick_rate: rate.rate(),
        });
        app.add_plugins(NetProtocolPlugin {
            capabilities: self.packages.manifest().capabilities,
        });
        app.insert_resource(PredictionSetup::default());
        let world = app.world_mut();
        let packages = &self.packages;
        // A client runs no scripts: it predicts only its own player's units.
        let setup = MatchSetup::new(PREDICTION_SEED, rate, Role::Client);
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
        world.init_resource::<KnownCommanded>();
        if let Some(data) = &self.data {
            world.insert_resource(ReceiptWriter::start(Arc::clone(data)));
        }
        if let Some(script) = &self.bot {
            app.insert_resource(script.clone());
            if let Some(after) = script.end() {
                app.insert_resource(BotLeave::new(after));
                app.add_systems(Update, BotLeave::leave);
            }
        }
        app.init_resource::<PendingOrders>();
        app.init_resource::<PendingSaves>();
        app.init_resource::<WorkerFaults>();
        app.add_observer(lose_link);
        // The systems on the join state run in the order of a link's life, as one frame may bring
        // an offer and the match's start; none reads what another's commands make, so the order
        // needs no sync point. A death of the frame logs at the tick of the join state they leave.
        // The faults of the frame apply last. The frame goes after the stack's own, as a server's
        // does (02-engine-core.md, The server's frame declares its order).
        let link = (
            retry_link,
            send_leave.run_if(resource_exists::<LeaveRequest>),
            send_saves,
            send_watch_teams.run_if(resource_changed::<JoinState>),
            receive_loaded.after(receive_input_acks),
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
                (
                    link,
                    receive_input_acks,
                    report_end,
                    report_deaths.after(receive_receipt),
                ),
                ReceiptWriter::report_failure,
                WorkerFaults::watch,
            )
                .chain()
                .after(InterpolationSystems::All),
        );
        app.add_systems(
            Last,
            ReceiptWriter::close_at_exit.run_if(on_message::<AppExit>),
        );
        app.add_systems(First, log_frame);
        app.add_systems(
            PreUpdate,
            follow_watched_tick.after(ReplicationSystems::Receive),
        );
        app.add_systems(
            PreUpdate,
            rewrite_after_rollback
                .after(RollbackSystems::Prepare)
                .before(RollbackSystems::Rollback)
                .run_if(Rollback::is_active),
        );
        // A frame's orders go out once, stamped for its last tick, the one it predicts: a frame
        // that runs several ticks sends at its end, so an order stamped for its first would lose
        // the lead of the ticks after it (11-net-stack/time.md, An input is stamped for the
        // frame's last tick).
        app.add_systems(
            FixedUpdate,
            (
                send_orders
                    .run_if(not(Rollback::is_active))
                    .run_if(last_tick_of_frame),
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
    mut receivers: Query<'_, '_, &mut MessageReceiver<SessionOffer>, With<Client>>,
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
                StartEffect::No => {}
                StartEffect::Playing { discarded } => {
                    MatchStarted {
                        player_slot: start.player_slot,
                        start_tick: start.clock.start().0,
                    }
                    .log();
                    if discarded > 0 {
                        InputsDiscarded {
                            player_slot: start.player_slot,
                            count: discarded,
                        }
                        .log();
                    }
                }
                StartEffect::Rewritten => {
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

/// Ends the match of a client whose session went back to a save, its link kept, as
/// `JoinState::load` says: it drops the inputs it sent and the orders it holds, as the load drops
/// those past the save, and answers the next offer. The replication despawns the units the load
/// took back, and sends those it restored.
fn receive_loaded(
    mut receivers: Query<'_, '_, &mut MessageReceiver<MatchLoaded>, With<Client>>,
    hosting: Res<'_, Hosting>,
    mut state: ResMut<'_, JoinState>,
    mut held: HeldInputs<'_>,
) {
    for mut receiver in &mut receivers {
        for MatchLoaded { tick } in receiver.receive() {
            if state.load(*hosting) {
                info!(
                    tick = tick.get(),
                    "the session went back to a save; joining again"
                );
                held.clear();
            }
        }
    }
}

/// Notes a link that failed: a client that knows the server's times drops the inputs it sent and
/// the orders it holds, and tries again; one that does not stops, and logs the cause its link's end
/// gives. The network stack despawns the units the link replicated.
fn lose_link(
    unlinked: On<'_, '_, Insert<Unlinked>>,
    clients: Query<'_, '_, &Unlinked, With<Client>>,
    clock: Res<'_, Clock>,
    os: Res<'_, Os>,
    mut state: ResMut<'_, JoinState>,
    mut held: HeldInputs<'_>,
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
            held.clear();
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
        let code = CloseCode::from(dismissed);
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

/// Tells the server the teams a spectator chose since its join, once it watches.
fn send_watch_teams(
    mut sender: Single<'_, '_, &mut MessageSender<WatchTeams>, With<Client>>,
    mut state: ResMut<'_, JoinState>,
) {
    // The mark of what the server holds is no change a system reads, and would run this again.
    if let Some(teams) = state.bypass_change_detection().untold_teams() {
        sender.send::<JoinChannel>(WatchTeams(teams));
    }
}

/// Watches the match the server's start names, for a client that answered to watch; see
/// `JoinState::watch`.
fn receive_watch_start(
    mut receivers: Query<'_, '_, &mut MessageReceiver<WatchStart>, With<Client>>,
    mut state: ResMut<'_, JoinState>,
) {
    for mut receiver in &mut receivers {
        for start in receiver.receive() {
            if state.watch(start) {
                info!(first = start.clock.first().get(), "watching the match");
            }
        }
    }
}

/// Sends the player's save commands while the client plays or watches: a spectator's report asks
/// a replay for the log it replays, and a server takes no other command of a spectator.
fn send_saves(
    mut sender: Single<'_, '_, &mut MessageSender<SaveCommand>, With<Client>>,
    state: Res<'_, JoinState>,
    mut pending: ResMut<'_, PendingSaves>,
) {
    if state.session().is_none() {
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
    if let Some(tick) = state.sim_tick(timeline.as_ref().map(|timeline| timeline.tick())) {
        ClientFrame { tick }.log();
    }
}

/// The client runs a sim tick in this fixed tick; see `JoinState::sim_tick`. A match start's
/// clock names the server's network tick, which a server started again counts from 0, while the
/// client's timeline counts the old link's until its first sync on the new one shifts it.
fn is_playing(state: Res<'_, JoinState>, timeline: Option<SyncedLocalTimeline<'_, '_>>) -> bool {
    state
        .sim_tick(timeline.as_ref().map(|timeline| timeline.tick()))
        .is_some()
}

/// Takes the server's word on where each of the player's inputs takes effect, so a rollback
/// replays it there.
fn receive_input_acks(
    mut receivers: Query<'_, '_, &mut MessageReceiver<InputAck>, With<Client>>,
    mut sent: ResMut<'_, SentInputs>,
    mut known: ResMut<'_, KnownCommanded>,
) {
    for mut receiver in &mut receivers {
        for ack in receiver.receive() {
            sent.acknowledge(ack.first, &ack.applied);
            known.acknowledge(ack.first, &ack.applied);
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
        Option<&'static UnitTags>,
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
    let tick = state.sim_tick(timeline.as_ref().map(|timeline| timeline.tick()));
    for (&id, team, owner, avatar, tags) in &died {
        let fallen = Fallen {
            id,
            team: team.copied(),
            owner: owner.map(|owner| owner.player_slot()),
            avatar,
            tags: tags.map_or(TagSet::NONE, |tags| tags.tags),
        };
        UnitDied::learned(tick, fallen).log();
    }
}

/// Adds the bot script's mode inputs due by the tick about to run, and its orders, each for the
/// player's units it names: one for the player's own avatar once the client predicts it, as a link
/// that came back replicates it a little later, so an order due before waits with those after it,
/// and goes out late; one whose units the player lacks is dropped. Then stamps the pending orders
/// with that tick, up to the session's max inputs per tick, chains each and keeps it, and sends
/// them in one message signed over the chain head after the last, as `PendingOrders::stamp` stamps
/// them. The inputs past the max wait for the next tick, so the log never refuses the message. The
/// inputs older than the deepest rollback the network stack takes are dropped first.
#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system takes each resource it reads"
)]
fn send_orders(
    timeline: Res<'_, LocalTimeline>,
    prediction: Res<'_, PredictionSetup>,
    timeline_config: Res<'_, InputTimelineSetup>,
    bot: Option<ResMut<'_, BotScript>>,
    view: ClientBotView<'_, '_>,
    mut scratch: Local<'_, OrderScratch>,
    mut orders: Local<'_, Vec<Order>>,
    signer: Res<'_, PlayerSigner>,
    mut pending: ResMut<'_, PendingOrders>,
    mut state: ResMut<'_, JoinState>,
    mut sent: ResMut<'_, SentInputs>,
    mut known: ResMut<'_, KnownCommanded>,
    mut order_payloads: Local<'_, OrderPayloads>,
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
        let player_slot = playing.chain.player_slot();
        let avatar = view.avatar(player_slot).is_some();
        let ready = |order: &BotOrder| avatar || order.units != BotUnits::Avatar;
        for scripted in bot.due_orders_while(stamp, ready) {
            orders.clear();
            let unsent = scripted.orders(player_slot, &view, &mut scratch, &mut orders);
            for order in orders.drain(..) {
                pending.push(order);
            }
            if let Some(cause) = unsent {
                BotOrderUnsent {
                    player_slot,
                    cause,
                    directive: scripted.named.clone(),
                }
                .log();
            }
        }
    }
    let session = playing.member.session;
    // The server refuses a packet whose inputs would apply past the max lead, as the inputs
    // before them fill each tick up to it, and ends the link: the inputs wait here while the
    // server may hold that many (05-protocol-spec.md, A late input applies at the next tick).
    let window = (lead.get() + 1) * u64::from(session.inputs.max_per_tick);
    let held = u64::try_from(sent.pending(server.unwrap_or(Tick::ZERO))).expect("a count fits");
    let room = window
        .saturating_sub(held)
        .min(u64::from(session.inputs.max_per_tick));
    let first = sent.len();
    pending.stamp(Stamping {
        stamp,
        next_seq: playing.chain.next_seq(),
        room: u32::try_from(room).expect("at most the max inputs a tick"),
        max_len: session.inputs.max_payload_len,
        sent: &mut sent,
        known: &mut known,
        payloads: &mut order_payloads,
    });
    let payloads = sent.since(first);
    // The message keeps the terms, as the server refuses one that does not and ends the link, so
    // a client's own fault crashes here rather than there.
    debug_assert!(
        u32::try_from(payloads.len()).is_ok_and(|count| count <= session.inputs.max_per_tick),
        "a stamp holds the max inputs a tick at most"
    );
    if payloads.len() == 0 {
        return;
    }
    OrdersSent {
        player_slot: playing.chain.player_slot(),
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

/// Whether the fixed tick that runs is its frame's last: the frame's time left is under a tick,
/// as Bevy's loop takes each tick's time before it runs the tick.
fn last_tick_of_frame(time: Res<'_, Time<Fixed>>) -> bool {
    time.overstep() < time.timestep()
}

/// Holds a watching client's sim tick at the state the server sent it last: the tick after the
/// one the server ran by its newest message, as a playing client's runs with its timeline, so
/// what reads the tick, a report and the HUD's clock, stands at what the client draws.
fn follow_watched_tick(
    state: Res<'_, JoinState>,
    confirmed: Res<'_, ConfirmedTickMap>,
    mut tick: ResMut<'_, SimTick>,
) {
    let (Some(clock), Some(last)) = (state.watching_clock(), confirmed.newest_tick()) else {
        return;
    };
    let next = clock.next_after(last);
    if tick.start() != next {
        *tick = SimTick::new(next);
    }
}

/// Runs the sim tick of the current network tick with the player's inputs stamped for it; in a
/// rollback, the network stack has wound its timeline back, so the sim follows. The match's state,
/// which every tick reads, reaches the client on the match's entity: until it holds it, the client
/// predicts nothing, as it holds no unit either.
fn run_predicted_tick(world: &mut World) {
    let tick = world.resource::<LocalTimeline>().tick();
    let Some(playing) = world.resource::<JoinState>().playing() else {
        return;
    };
    if world.match_entity().is_none() {
        return;
    }
    let player_slot = playing.chain.player_slot();
    let Some(sim_tick) = playing.clock.sim_tick(tick) else {
        return;
    };
    *world.resource_mut::<SimTick>() = SimTick::new(sim_tick);
    world.resource_scope(|world, sent: Mut<'_, SentInputs>| {
        let mut inputs = world.resource_mut::<TickInputs>();
        for payload in sent.at(sim_tick) {
            inputs.push(TickInput {
                player_slot,
                payload,
            });
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

#[cfg(test)]
mod tests;
