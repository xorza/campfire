use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryState;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use bevy_time::{Real, Time};
use campfire_common::PlayerSlot;
use campfire_mode::ModeWorld;
use campfire_protocol::{Controller, Delegation, LeaveReason, ServerInput};
use campfire_quic::CloseLink;
use campfire_runner::{ServerInputRefused, Session, SlotRuleError};
use lightyear::prelude::{Connected, MessageReceiver, MessageSender};
use tracing::info;

use crate::close_code::CloseCode;
use crate::events::join_refused::error::JoinError;
use crate::join::Join;
use crate::leave_match::LeaveMatch;
use crate::match_clock::MatchClock;
use crate::match_start::{ChainHead, MatchStart};
use crate::net_protocol::MatchChannel;
use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::offering::{
    CheckedJoins, Dismissing, JoinLink, Joined, Offered, Offering, ToAnswer,
};
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::seats::{Seat, Seats};
use crate::sim_server::spectators::spectators_world::SpectatorsWorldCrate;

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

/// What the door seats a player in: the slot, and their chain as the log holds it, or none for a
/// chain that starts from their delegation.
#[derive(Debug, Clone, Copy)]
struct SeatIn {
    slot: PlayerSlot,
    chain: Option<ChainHead>,
}

impl AsRef<Offering> for Door {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

impl Door {
    pub(crate) const fn new(offering: Offering) -> Door {
        Door { offering }
    }

    /// Seats the player of each offered link that answered, or refuses them; the joins of one
    /// frame in `Offering::check_all`'s order.
    pub(crate) fn take_joins(
        world: &mut World,
        mut links: Local<'_, QueryState<JoinLink, ToAnswer>>,
        mut joins: Local<'_, Vec<(Entity, Offered, Join)>>,
        mut checked: Local<'_, CheckedJoins>,
    ) {
        joins.extend(links.iter_mut(world).filter_map(Offering::received));
        world
            .resource::<Door>()
            .offering
            .check_all(joins.drain(..), &mut checked);
        for (link, error) in checked.refused.drain(..) {
            Door::refuse(world, link, error);
        }
        let visibility = world.resource::<MatchVisibility>().0;
        for link in checked.watching.drain(..) {
            if let Err(error) = world.admit_spectator(link, &visibility) {
                Door::refuse(world, link, error);
            }
        }
        for (link, delegation) in checked.answered.drain(..) {
            match Door::seat_player(world, link, delegation) {
                Ok(seat) => Door::seat(world, link, seat),
                Err(error) => Door::refuse(world, link, error),
            }
        }
    }

    fn refuse(world: &mut World, link: Entity, error: JoinError) {
        Offering::refuse(&mut world.commands(), link, error);
        world.flush();
    }

    /// Logs what seating the player of `delegation` changes, and gives where they sit.
    fn seat_player(
        world: &mut World,
        link: Entity,
        delegation: Delegation,
    ) -> Result<SeatIn, JoinError> {
        let main = *delegation.main_key();
        let log = world.resource::<Session>().log();
        let slots = (0..log.slot_count()).map(PlayerSlot::new);
        let own = slots.clone().find_map(|slot| match log.controller(slot)? {
            Controller::Player {
                main_key,
                session_key,
                chain,
            } if main_key == main => Some((slot, session_key, chain)),
            _ => None,
        });
        if let Some((slot, session_key, chain)) = own {
            let renewed = session_key != delegation.terms().session_key;
            if renewed {
                Door::serve(world, ServerInput::Renew { slot, delegation })?;
            }
            let seat = world.resource::<Seats>().get(slot);
            match seat.link {
                Some(older) if older != link => {
                    info!(
                        slot = slot.get(),
                        ?older,
                        "a newer login took the player's slot"
                    );
                    Dismissing::refuse(&mut world.commands(), older, JoinError::Superseded);
                    world.flush();
                }
                _ if seat.gone_since.is_some() => {
                    Door::serve(world, ServerInput::Connected { slot })?;
                }
                _ => {}
            }
            let head = ChainHead {
                next_seq: chain.next_seq(),
                head: chain.head(),
                last_stamp: world.resource::<Session>().log().last_stamp(slot),
            };
            return Ok(SeatIn {
                slot,
                chain: Some(head),
            });
        }
        let log = world.resource::<Session>().log();
        let left = slots.clone().find(|&slot| {
            log.leaver(slot) == Some(main)
                && !matches!(log.controller(slot), Some(Controller::Player { .. }))
        });
        if let Some(slot) = left {
            Door::serve(world, ServerInput::Join { slot, delegation })?;
            return Ok(SeatIn { slot, chain: None });
        }
        for wanted in [Controller::Open, Controller::Bot] {
            let log = world.resource::<Session>().log();
            let free = slots
                .clone()
                .find(|&slot| log.controller(slot) == Some(wanted));
            if let Some(slot) = free {
                let join = ServerInput::Join {
                    slot,
                    delegation: delegation.clone(),
                };
                match Door::serve(world, join) {
                    Ok(()) => return Ok(SeatIn { slot, chain: None }),
                    Err(JoinError::Refused(ServerInputRefused::Rule(
                        SlotRuleError::LateJoin | SlotRuleError::BotTakeover,
                    ))) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Err(JoinError::NoSlot)
    }

    /// Seats `link` as `seat` says: it carries that slot's inputs, learns the match from the
    /// next tick on, the teams' relations and any end at once, predicts the slot's entities its
    /// kind lets its owner predict ([`KindShows`](crate::sim_server::kind_shows::KindShows)), and
    /// stands in its team's room, so it receives every unit its team sees and no other.
    fn seat(world: &mut World, link: Entity, seat: SeatIn) {
        let SeatIn { slot, chain } = seat;
        let team = world.team_of(slot).expect("every slot has a team");
        world
            .entity_mut(link)
            .insert((Joined, PlayerLink::new(slot, team)));
        world.resource_mut::<Seats>().set(
            slot,
            Seat {
                link: Some(link),
                gone_since: None,
            },
        );
        let log = world.resource::<Session>().log();
        let (first, loaded) = (log.next_tick(), log.loaded());
        let start_tick = world.resource::<MatchClock>().net_tick(first);
        world
            .get_mut::<MessageSender<MatchStart>>(link)
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

    /// Logs `input`, signed by the server.
    fn serve(world: &mut World, input: ServerInput<'_>) -> Result<(), JoinError> {
        world
            .resource_mut::<Session>()
            .serve(input)
            .map_err(JoinError::Refused)
    }

    /// Watches the seated links: a link that failed logs its slot `Disconnected`, and the grace
    /// period runs from then by the server's real clock; a slot whose player is still gone after
    /// it logs the player's `Leave` after grace; a player who asks to leave logs `Leave` at once,
    /// and their link ends.
    pub(crate) fn watch(world: &mut World) {
        let now = world.resource::<Time<Real>>().elapsed();
        let grace = world.resource::<Door>().offering.times.grace;
        let slots = world.resource::<Session>().log().slot_count();
        for slot in (0..slots).map(PlayerSlot::new) {
            let seat = world.resource::<Seats>().get(slot);
            if let Some(link) = seat.link {
                if Door::asked_to_leave(world, link) {
                    Door::leave(world, slot, LeaveReason::Asked);
                    world.resource_mut::<Seats>().set(slot, Seat::default());
                    if let Ok(mut gone) = world.get_entity_mut(link) {
                        gone.remove::<PlayerLink>();
                    }
                    world.trigger(CloseLink {
                        entity: link,
                        code: CloseCode::Left.app(),
                    });
                    continue;
                }
                let live = world
                    .get_entity(link)
                    .is_ok_and(|link| link.contains::<Connected>());
                if live {
                    continue;
                }
                info!(
                    slot = slot.get(),
                    "a player's link failed; their grace period runs"
                );
                Door::serve(world, ServerInput::Disconnected { slot })
                    .expect("a seated slot has its player");
                if let Ok(mut gone) = world.get_entity_mut(link) {
                    gone.remove::<PlayerLink>();
                }
                let gone = Seat {
                    link: None,
                    gone_since: Some(now),
                };
                world.resource_mut::<Seats>().set(slot, gone);
            } else if let Some(since) = seat.gone_since
                && now.saturating_sub(since) >= grace
            {
                info!(slot = slot.get(), "a player's grace period passed");
                let player = matches!(
                    world.resource::<Session>().log().controller(slot),
                    Some(Controller::Player { .. })
                );
                if player {
                    Door::leave(world, slot, LeaveReason::Grace);
                }
                world.resource_mut::<Seats>().set(slot, Seat::default());
            }
        }
    }

    /// Whether `link` sent the word that its player leaves.
    fn asked_to_leave(world: &mut World, link: Entity) -> bool {
        world
            .get_mut::<MessageReceiver<LeaveMatch>>(link)
            .is_some_and(|mut receiver| receiver.receive().count() > 0)
    }

    /// Logs the leave of `slot`'s player for `reason`, the slot becoming what the mode says.
    fn leave(world: &mut World, slot: PlayerSlot, reason: LeaveReason) {
        let becomes = world.resource::<Session>().after_leave();
        info!(slot = slot.get(), ?reason, ?becomes, "a player left");
        Door::serve(
            world,
            ServerInput::Leave {
                slot,
                reason,
                becomes,
            },
        )
        .expect("a player's slot takes their leave");
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
