use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut, SystemParam, SystemState};
use bevy_ecs::world::World;
use bevy_time::{Real, Time};
use campfire_common::ErrorReport;
use campfire_common::Seat;
use campfire_mode::MatchMode;
use campfire_protocol::{
    Controller, Delegation, LeaveReason, ServerInput, ServerInputError, SessionLog,
};
use campfire_quic::CloseLink;
use campfire_runner::{SeatRuleError, ServerInputRefusal, Session};
use lightyear::prelude::{Connected, MessageReceiver, MessageSender};
use tracing::info;

use crate::close_code::CloseCode;
use crate::error::JoinError;
use crate::match_clock::MatchClock;
use crate::net_protocol::MatchChannel;
use crate::net_protocol::leave_match::LeaveMatch;
use crate::net_protocol::match_start::{ChainHead, MatchStart, SeatChain};
use crate::net_protocol::watch_start::WatchStart;
use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::offering::{CheckedJoins, Dismissing, JoinLinks, Joined, Offering};
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::seat_links::{SeatLink, SeatLinks};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::viewer::Viewer;

/// The session's door after the start: it offers the terms to every new link, and seats each
/// player who answers. A main key that controls a seat gets it back, its live link ended if it
/// has one, as the newest login wins; one that left a seat still reserved, played by a bot or
/// open gets it back too; any other gets an open seat when the mode lets a player join late,
/// else a bot's when it lets a player take a bot's over, else a refusal. It also watches the
/// seated links: a player whose link fails keeps their seat for the grace period, and leaves it
/// after, or at once when they ask.
#[derive(Resource, Debug)]
pub(crate) struct Door {
    offering: Offering,
}

/// What the door seats a player in: the seat, and the chain they sign.
#[derive(Debug, Clone, Copy)]
struct SeatIn {
    seat: Seat,
    chain: SeatChain,
}

impl AsRef<Offering> for Door {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

/// What the door seats a player with: the session, which logs what seating them changes, the
/// seats, the mode, which gives a seat's team, the match's clock, and each link's sender of the
/// match's start.
#[derive(SystemParam)]
pub(crate) struct Seating<'w, 's> {
    commands: Commands<'w, 's>,
    session: ResMut<'w, Session>,
    seat_links: ResMut<'w, SeatLinks>,
    mode: Res<'w, MatchMode>,
    clock: Res<'w, MatchClock>,
    starts: Query<'w, 's, &'static mut MessageSender<MatchStart>>,
}

impl Door {
    pub(crate) const fn new(offering: Offering) -> Self {
        Self { offering }
    }

    /// Seats the player of each offered link that answered, or refuses them, and admits each
    /// spectator while a place is free; the joins of one frame in `Offering::check_all`'s order.
    pub(crate) fn take_joins(
        mut door: ResMut<'_, Self>,
        mut links: JoinLinks<'_, '_>,
        viewers: Query<'_, '_, &Viewer>,
        visibility: Res<'_, MatchVisibility>,
        mut watch_starts: Query<'_, '_, &mut MessageSender<WatchStart>>,
        mut seating: Seating<'_, '_>,
        mut checked: Local<'_, CheckedJoins>,
    ) {
        let joins = links.iter_mut().filter_map(Offering::received);
        door.offering.check_all(joins, &mut checked);
        for (link, error) in checked.refused.drain(..) {
            Offering::refuse(&mut seating.commands, link, error);
        }
        let mut watching = Spectators::count(viewers.iter());
        let first = seating.session.log().next_tick();
        for link in checked.watching.drain(..) {
            match Spectators::place(&visibility.0, watching) {
                Ok(viewer) => {
                    seating.commands.entity(link).insert((Joined, viewer));
                    watching += 1;
                    let mut sender = watch_starts
                        .get_mut(link)
                        .expect("a client link sends the watch start");
                    Spectators::tell_start(&mut sender, &seating.clock, first, link);
                }
                Err(error) => Offering::refuse(&mut seating.commands, link, error),
            }
        }
        for (link, delegation) in checked.answered.drain(..) {
            match Self::seat_player(&mut seating, link, delegation) {
                Ok(seat_in) => Self::seat(&mut seating, link, seat_in),
                Err(error) => Offering::refuse(&mut seating.commands, link, error),
            }
        }
    }

    /// Logs what seating the player of `delegation` changes, and gives where they sit.
    fn seat_player(
        seating: &mut Seating<'_, '_>,
        link: Entity,
        delegation: Delegation,
    ) -> Result<SeatIn, JoinError> {
        let main = *delegation.main_key();
        let log = seating.session.log();
        let seats = (0..log.seat_count()).map(Seat::new);
        let own = seats.clone().find_map(|seat| match log.controller(seat)? {
            Controller::Player { delegation, chain } if *delegation.main_key() == main => {
                Some((seat, delegation.terms().session_key, chain))
            }
            _ => None,
        });
        if let Some((seat, session_key, chain)) = own {
            let renewed = session_key != delegation.terms().session_key;
            if renewed {
                Self::serve(
                    &mut seating.session,
                    ServerInput::Renew { seat, delegation },
                )?;
            }
            let seat_link = seating.seat_links.get(seat);
            match seat_link.link {
                Some(older) if older != link => {
                    info!(
                        seat = seat.get(),
                        ?older,
                        "a newer login took the player's seat"
                    );
                    Dismissing::refuse(&mut seating.commands, older, JoinError::Superseded);
                }
                _ if seat_link.gone_since.is_some() => {
                    Self::serve(&mut seating.session, ServerInput::Connected { seat })?;
                }
                _ => {}
            }
            let log = seating.session.log();
            let head = ChainHead {
                next_seq: chain.next_seq(),
                head: chain.head(),
                last_stamp: log.last_stamp(seat),
            };
            return Ok(SeatIn {
                seat,
                chain: SeatChain::Held(head),
            });
        }
        let log = seating.session.log();
        let left = seats.clone().find(|&seat| {
            log.leaver(seat) == Some(main)
                && !matches!(log.controller(seat), Some(Controller::Player { .. }))
        });
        if let Some(seat) = left {
            Self::serve(&mut seating.session, ServerInput::Join { seat, delegation })?;
            return Ok(Self::joined(seating.session.log(), seat));
        }
        for wanted in [Controller::Open, Controller::Bot] {
            let log = seating.session.log();
            let free = seats
                .clone()
                .find(|&seat| log.controller(seat) == Some(wanted));
            if let Some(seat) = free {
                let join = ServerInput::Join {
                    seat,
                    delegation: delegation.clone(),
                };
                match Self::serve(&mut seating.session, join) {
                    Ok(()) => return Ok(Self::joined(seating.session.log(), seat)),
                    Err(JoinError::Refused(ServerInputRefusal::Rule(
                        SeatRuleError::LateJoin | SeatRuleError::BotTakeover,
                    ))) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Err(JoinError::NoSeat)
    }

    /// Seats each of `links` in its seat, which the log gave its player, in its new chain, as
    /// the match starts.
    pub(super) fn seat_new(world: &mut World, links: &[(Seat, Entity)]) {
        let mut state = SystemState::<Seating<'_, '_>>::new(world);
        let mut seating = state
            .get_mut(world)
            .expect("a match that begins holds what its door seats players with");
        for &(seat, link) in links {
            let seat_in = Self::joined(seating.session.log(), seat);
            Self::seat(&mut seating, link, seat_in);
        }
        state.apply(world);
    }

    /// Where the player whose join of `seat` `log` just took sits: in its new chain.
    fn joined(log: &SessionLog, seat: Seat) -> SeatIn {
        SeatIn {
            seat,
            chain: SeatChain::new_of(log, seat),
        }
    }

    /// Seats `link` as `seat` says: it carries that seat's inputs, learns the match from the
    /// next tick on, the teams' relations and any end at once, predicts the seat's entities its
    /// kind lets its owner predict ([`KindShows`](crate::sim_server::kind_shows::KindShows)), and
    /// stands in its team's room, so it receives every unit its team sees and no other.
    fn seat(seating: &mut Seating<'_, '_>, link: Entity, seat_in: SeatIn) {
        let SeatIn { seat, chain } = seat_in;
        let team = seating.mode.team_of(seat).expect("every seat has a team");
        seating
            .commands
            .entity(link)
            .insert((Joined, PlayerLink::new(seat, team)));
        seating.seat_links.set(
            seat,
            SeatLink {
                link: Some(link),
                gone_since: None,
            },
        );
        let log = seating.session.log();
        let (first, loaded) = (log.next_tick(), log.is_loaded());
        let start_tick = seating.clock.net_tick(first);
        seating
            .starts
            .get_mut(link)
            .expect("a client link sends the match start")
            .send::<MatchChannel>(MatchStart {
                start_tick: start_tick.0,
                first,
                seat,
                team,
                chain,
                loaded,
            });
        info!(
            seat = seat.get(),
            link = link.to_bits(),
            "a player took their seat"
        );
    }

    /// Logs `input` into `session`, which signs it.
    fn serve(session: &mut Session, input: ServerInput<'_>) -> Result<(), JoinError> {
        session.serve(input).map_err(JoinError::Refused)
    }

    /// Watches the seated links: a link that failed logs its seat `Disconnected`, and the grace
    /// period runs from then by the server's real clock; a seat whose player is still gone after
    /// it logs the player's `Leave` after grace; a player who asks to leave logs `Leave` at once,
    /// and their link ends.
    pub(crate) fn watch(
        time: Res<'_, Time<Real>>,
        door: Res<'_, Self>,
        mut session: ResMut<'_, Session>,
        mut seat_links: ResMut<'_, SeatLinks>,
        mut leaving: Query<'_, '_, &mut MessageReceiver<LeaveMatch>>,
        connected: Query<'_, '_, (), With<Connected>>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        let grace = door.offering.times.grace;
        let seats = session.log().seat_count();
        for seat in (0..seats).map(Seat::new) {
            let seat_link = seat_links.get(seat);
            if let Some(link) = seat_link.link {
                let asked = leaving
                    .get_mut(link)
                    .is_ok_and(|mut receiver| receiver.receive().count() > 0);
                if asked {
                    Self::leave(&mut session, seat, LeaveReason::Asked);
                    seat_links.set(seat, SeatLink::default());
                    if let Ok(mut gone) = commands.get_entity(link) {
                        gone.try_remove::<PlayerLink>();
                    }
                    commands.trigger(CloseLink {
                        entity: link,
                        code: CloseCode::Left.app(),
                    });
                    continue;
                }
                if connected.contains(link) {
                    continue;
                }
                info!(
                    seat = seat.get(),
                    "a player's link failed; their grace period runs"
                );
                Self::serve_unless_full(&mut session, ServerInput::Disconnected { seat });
                if let Ok(mut gone) = commands.get_entity(link) {
                    gone.try_remove::<PlayerLink>();
                }
                let gone = SeatLink {
                    link: None,
                    gone_since: Some(now),
                };
                seat_links.set(seat, gone);
            } else if let Some(since) = seat_link.gone_since
                && now.saturating_sub(since) >= grace
            {
                info!(seat = seat.get(), "a player's grace period passed");
                let player = matches!(
                    session.log().controller(seat),
                    Some(Controller::Player { .. })
                );
                if player {
                    Self::leave(&mut session, seat, LeaveReason::Grace);
                }
                seat_links.set(seat, SeatLink::default());
            }
        }
    }

    /// Logs the leave of `seat`'s player for `reason`, the seat becoming what the mode says.
    fn leave(session: &mut Session, seat: Seat, reason: LeaveReason) {
        let becomes = session.after_leave();
        info!(seat = seat.get(), ?reason, ?becomes, "a player left");
        Self::serve_unless_full(
            session,
            ServerInput::Leave {
                seat,
                reason,
                becomes,
            },
        );
    }

    /// Logs `input`, a change of a link or a leave, which its seat's player always allows,
    /// into `session`, which signs it; none when the log is full, as the session then ends at
    /// the next boundary, before the tick it would apply in.
    pub(crate) fn serve_unless_full(session: &mut Session, input: ServerInput<'_>) {
        match Self::serve(session, input) {
            Ok(())
            | Err(JoinError::Refused(ServerInputRefusal::Log(ServerInputError::LogFull))) => {}
            Err(error) => panic!("the server's own input holds: {}", ErrorReport::of(&error)),
        }
    }

    /// Whether every seat a player controlled has left, and no player holds a link: the session
    /// then ends.
    pub(super) fn is_empty(world: &World) -> bool {
        let none_held = world
            .resource::<Session>()
            .log()
            .player_seats()
            .next()
            .is_none();
        none_held
            && world
                .resource::<SeatLinks>()
                .iter()
                .all(|(_, seat_link)| seat_link.link.is_none())
    }
}
