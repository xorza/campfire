use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Res};
use bevy_ecs::world::{Mut, World};
use bevy_time::{Real, Time};
use campfire_capabilities::{Area, MatchEnd, Mode, Owner, Projectile, Relations};
use campfire_common::PlayerSlot;
use campfire_log::{ErrorReport, LogEvent};
use campfire_protocol::{ConnectChallenge, Controller, Delegation, LeaveReason, ServerInput};
use campfire_runner::{ServerInputRefused, Session, SlotRuleError};
use lightyear::prelude::{
    Connected, LocalTimeline, MessageReceiver, MessageSender, PredictionTarget, Replicate, Unlink,
    UnlinkReason,
};
use tracing::info;

use crate::events::join_refused::JoinRefused;
use crate::join::Join;
use crate::leave_match::LeaveMatch;
use crate::match_clock::MatchClock;
use crate::match_start::{ChainHead, MatchStart};
use crate::net_protocol::MatchChannel;
use crate::sim_server::error::JoinError;
use crate::sim_server::offering::{
    JoinLink, Joined, OfferLinks, Offering, Refused, Superseding, Unanswered,
};
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::seats::{Seat, Seats};
use crate::sim_server::server_signer::ServerSigner;

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

impl Door {
    pub(crate) const fn new(offering: Offering) -> Door {
        Door { offering }
    }

    /// Offers the terms to each connected link that has no offer; see `Offering::offer`.
    pub(crate) fn offer(
        door: Res<'_, Door>,
        timeline: Res<'_, LocalTimeline>,
        mut links: OfferLinks<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        door.offering
            .offer(timeline.tick(), &mut links, &mut commands);
    }

    /// Seats the player of each offered link that answered, or refuses them.
    pub(crate) fn take_joins(
        world: &mut World,
        mut links: Local<'_, QueryState<JoinLink, Unanswered>>,
    ) {
        let joins: Vec<(Entity, ConnectChallenge, Join)> = links
            .iter_mut(world)
            .filter_map(|(link, offered, mut receiver)| {
                let join = receiver.receive().next()?;
                Some((link, offered.challenge, join))
            })
            .collect();
        for (link, challenge, join) in joins {
            let delegation = world.resource::<Door>().offering.check(challenge, &join);
            match delegation.and_then(|delegation| Door::seat_player(world, link, delegation)) {
                Ok(seat) => Door::seat(world, link, seat),
                Err(error) => {
                    JoinRefused {
                        link: format!("{link:?}"),
                        error: ErrorReport::of(&error).to_string(),
                    }
                    .log();
                    world.entity_mut(link).insert(Refused(error));
                }
            }
        }
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
                    Superseding::mark(&mut world.commands(), older);
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
    /// next tick on, the teams' relations and any end at once, predicts the slot's units, and
    /// stands in its team's room, so it receives every unit its team sees and no other.
    fn seat(world: &mut World, link: Entity, seat: SeatIn) {
        let SeatIn { slot, chain } = seat;
        let team = Mode::team_of(world, slot).expect("every slot has a team");
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
        let relations = world.resource::<Relations>().clone();
        world
            .get_mut::<MessageSender<Relations>>(link)
            .expect("a client link sends the relations")
            .send::<MatchChannel>(relations);
        if let Some(&end) = world.get_resource::<MatchEnd>() {
            world
                .get_mut::<MessageSender<MatchEnd>>(link)
                .expect("a client link sends the match's end")
                .send::<MatchChannel>(end);
        }
        let mut units = world.query_filtered::<(
            Entity,
            Option<&Owner>,
            Has<Projectile>,
            Has<Area>,
        ), With<Replicate>>();
        let owned: Vec<Entity> = units
            .iter(world)
            .filter(|&(_, owner, projectile, area)| {
                owner.is_some_and(|owner| owner.slot() == slot) && !projectile && !area
            })
            .map(|(unit, ..)| unit)
            .collect();
        let mut commands = world.commands();
        for unit in owned {
            commands
                .entity(unit)
                .insert(PredictionTarget::manual(vec![link]));
        }
        world.flush();
        info!(slot = slot.get(), ?link, "a player took their seat");
    }

    /// Logs `input`, signed by the server.
    fn serve(world: &mut World, input: ServerInput<'_>) -> Result<(), JoinError> {
        world.resource_scope(|world, mut session: Mut<'_, Session>| {
            world
                .resource::<ServerSigner>()
                .serve(&mut session, input)
                .map_err(JoinError::Refused)
        })
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
                    world.trigger(Unlink {
                        entity: link,
                        reason: UnlinkReason::UserRequested(Some("the player left".to_owned())),
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
    pub(crate) fn empty(world: &World) -> bool {
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
