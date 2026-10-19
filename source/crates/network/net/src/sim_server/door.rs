use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut, SystemParam, SystemState};
use bevy_ecs::world::World;
use bevy_time::{Real, Time};
use campfire_common::ErrorReport;
use campfire_common::PlayerSlot;
use campfire_mode::MatchMode;
use campfire_protocol::{
    Controller, Delegation, LeaveReason, ServerInput, ServerInputError, SessionLog,
};
use campfire_quic::CloseLink;
use campfire_runner::{ServerInputRefused, Session, SlotRuleError};
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
use crate::sim_server::seats::{Seat, Seats};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::viewer::Viewer;

/// The session's door after the start: it offers the terms to every new link, and seats each
/// player who answers. A main key that controls a slot gets it back, its live link ended if it
/// has one, as the newest login wins; one that left a slot still reserved, played by a bot or
/// open gets it back too; any other gets an open slot when the mode lets a player join late,
/// else a bot's when it lets a player take a bot's over, else a refusal. It also watches the
/// seated links: a player whose link fails keeps their slot for the grace period, and leaves it
/// after, or at once when they ask.
#[derive(Resource, Debug)]
pub(crate) struct Door {
    offering: Offering,
}

/// What the door seats a player in: the slot, and the chain they sign.
#[derive(Debug, Clone, Copy)]
struct SeatIn {
    slot: PlayerSlot,
    chain: SeatChain,
}

impl AsRef<Offering> for Door {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

/// What the door seats a player with: the session, which logs what seating them changes, the
/// seats, the mode, which gives a slot's team, the match's clock, and each link's sender of the
/// match's start.
#[derive(SystemParam)]
pub(crate) struct Seating<'w, 's> {
    commands: Commands<'w, 's>,
    session: ResMut<'w, Session>,
    seats: ResMut<'w, Seats>,
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
                Ok(seat) => Self::seat(&mut seating, link, seat),
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
        let slots = (0..log.slot_count()).map(PlayerSlot::new);
        let own = slots.clone().find_map(|slot| match log.controller(slot)? {
            Controller::Player { delegation, chain } if *delegation.main_key() == main => {
                Some((slot, delegation.terms().session_key, chain))
            }
            _ => None,
        });
        if let Some((slot, session_key, chain)) = own {
            let renewed = session_key != delegation.terms().session_key;
            if renewed {
                Self::serve(
                    &mut seating.session,
                    ServerInput::Renew { slot, delegation },
                )?;
            }
            let seat = seating.seats.get(slot);
            match seat.link {
                Some(older) if older != link => {
                    info!(
                        slot = slot.get(),
                        ?older,
                        "a newer login took the player's slot"
                    );
                    Dismissing::refuse(&mut seating.commands, older, JoinError::Superseded);
                }
                _ if seat.gone_since.is_some() => {
                    Self::serve(&mut seating.session, ServerInput::Connected { slot })?;
                }
                _ => {}
            }
            let log = seating.session.log();
            let head = ChainHead {
                next_seq: chain.next_seq(),
                head: chain.head(),
                last_stamp: log.last_stamp(slot),
            };
            return Ok(SeatIn {
                slot,
                chain: SeatChain::Held(head),
            });
        }
        let log = seating.session.log();
        let left = slots.clone().find(|&slot| {
            log.leaver(slot) == Some(main)
                && !matches!(log.controller(slot), Some(Controller::Player { .. }))
        });
        if let Some(slot) = left {
            Self::serve(&mut seating.session, ServerInput::Join { slot, delegation })?;
            return Ok(Self::joined(seating.session.log(), slot));
        }
        for wanted in [Controller::Open, Controller::Bot] {
            let log = seating.session.log();
            let free = slots
                .clone()
                .find(|&slot| log.controller(slot) == Some(wanted));
            if let Some(slot) = free {
                let join = ServerInput::Join {
                    slot,
                    delegation: delegation.clone(),
                };
                match Self::serve(&mut seating.session, join) {
                    Ok(()) => return Ok(Self::joined(seating.session.log(), slot)),
                    Err(JoinError::Refused(ServerInputRefused::Rule(
                        SlotRuleError::LateJoin | SlotRuleError::BotTakeover,
                    ))) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Err(JoinError::NoSlot)
    }

    /// Seats each of `links` in its slot, which the log gave its player, in its new chain, as
    /// the match starts.
    pub(super) fn seat_new(world: &mut World, links: &[(PlayerSlot, Entity)]) {
        let mut state = SystemState::<Seating<'_, '_>>::new(world);
        let mut seating = state
            .get_mut(world)
            .expect("a match that begins holds what its door seats players with");
        for &(slot, link) in links {
            let seat = Self::joined(seating.session.log(), slot);
            Self::seat(&mut seating, link, seat);
        }
        state.apply(world);
    }

    /// Where the player whose join of `slot` `log` just took sits: in its new chain.
    fn joined(log: &SessionLog, slot: PlayerSlot) -> SeatIn {
        SeatIn {
            slot,
            chain: SeatChain::new_of(log, slot),
        }
    }

    /// Seats `link` as `seat` says: it carries that slot's inputs, learns the match from the
    /// next tick on, the teams' relations and any end at once, predicts the slot's entities its
    /// kind lets its owner predict ([`KindShows`](crate::sim_server::kind_shows::KindShows)), and
    /// stands in its team's room, so it receives every unit its team sees and no other.
    fn seat(seating: &mut Seating<'_, '_>, link: Entity, seat: SeatIn) {
        let SeatIn { slot, chain } = seat;
        let team = seating.mode.team_of(slot).expect("every slot has a team");
        seating
            .commands
            .entity(link)
            .insert((Joined, PlayerLink::new(slot, team)));
        seating.seats.set(
            slot,
            Seat {
                link: Some(link),
                gone_since: None,
            },
        );
        let log = seating.session.log();
        let (first, loaded) = (log.next_tick(), log.loaded());
        let start_tick = seating.clock.net_tick(first);
        seating
            .starts
            .get_mut(link)
            .expect("a client link sends the match start")
            .send::<MatchChannel>(MatchStart {
                start_tick: start_tick.0,
                first,
                slot,
                team,
                chain,
                loaded,
            });
        info!(
            slot = slot.get(),
            link = link.to_bits(),
            "a player took their seat"
        );
    }

    /// Logs `input` into `session`, which signs it.
    fn serve(session: &mut Session, input: ServerInput<'_>) -> Result<(), JoinError> {
        session.serve(input).map_err(JoinError::Refused)
    }

    /// Watches the seated links: a link that failed logs its slot `Disconnected`, and the grace
    /// period runs from then by the server's real clock; a slot whose player is still gone after
    /// it logs the player's `Leave` after grace; a player who asks to leave logs `Leave` at once,
    /// and their link ends.
    pub(crate) fn watch(
        time: Res<'_, Time<Real>>,
        door: Res<'_, Self>,
        mut session: ResMut<'_, Session>,
        mut seats: ResMut<'_, Seats>,
        mut leaving: Query<'_, '_, &mut MessageReceiver<LeaveMatch>>,
        connected: Query<'_, '_, (), With<Connected>>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        let grace = door.offering.times.grace;
        let slots = session.log().slot_count();
        for slot in (0..slots).map(PlayerSlot::new) {
            let seat = seats.get(slot);
            if let Some(link) = seat.link {
                let asked = leaving
                    .get_mut(link)
                    .is_ok_and(|mut receiver| receiver.receive().count() > 0);
                if asked {
                    Self::leave(&mut session, slot, LeaveReason::Asked);
                    seats.set(slot, Seat::default());
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
                    slot = slot.get(),
                    "a player's link failed; their grace period runs"
                );
                Self::serve_unless_full(&mut session, ServerInput::Disconnected { slot });
                if let Ok(mut gone) = commands.get_entity(link) {
                    gone.try_remove::<PlayerLink>();
                }
                let gone = Seat {
                    link: None,
                    gone_since: Some(now),
                };
                seats.set(slot, gone);
            } else if let Some(since) = seat.gone_since
                && now.saturating_sub(since) >= grace
            {
                info!(slot = slot.get(), "a player's grace period passed");
                let player = matches!(
                    session.log().controller(slot),
                    Some(Controller::Player { .. })
                );
                if player {
                    Self::leave(&mut session, slot, LeaveReason::Grace);
                }
                seats.set(slot, Seat::default());
            }
        }
    }

    /// Logs the leave of `slot`'s player for `reason`, the slot becoming what the mode says.
    fn leave(session: &mut Session, slot: PlayerSlot, reason: LeaveReason) {
        let becomes = session.after_leave();
        info!(slot = slot.get(), ?reason, ?becomes, "a player left");
        Self::serve_unless_full(
            session,
            ServerInput::Leave {
                slot,
                reason,
                becomes,
            },
        );
    }

    /// Logs `input`, a change of a link or a leave, which its slot's player always allows,
    /// into `session`, which signs it; none when the log is full, as the session then ends at
    /// the next boundary, before the tick it would apply in.
    pub(crate) fn serve_unless_full(session: &mut Session, input: ServerInput<'_>) {
        match Self::serve(session, input) {
            Ok(())
            | Err(JoinError::Refused(ServerInputRefused::Log(ServerInputError::LogFull))) => {}
            Err(error) => panic!("the server's own input holds: {}", ErrorReport::of(&error)),
        }
    }

    /// Whether every slot a player controlled has left, and no player holds a link: the session
    /// then ends.
    pub(super) fn empty(world: &World) -> bool {
        let none_held = world
            .resource::<Session>()
            .log()
            .player_slots()
            .next()
            .is_none();
        none_held
            && world
                .resource::<Seats>()
                .iter()
                .all(|(_, seat)| seat.link.is_none())
    }
}
