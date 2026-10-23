use std::fmt;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_common::{ErrorReport, PlayerSlot, Tick};
use campfire_declarations::MatchFacts;
use campfire_mode::MatchMode;
use campfire_net_connection::{Connected, Disconnect};
use campfire_net_messages::{MessageReceiver, MessageSender};
use campfire_net_timeline::Clock;
use campfire_protocol::{
    Controller, Delegation, LeaveReason, ServerInput, ServerInputError, SessionLog,
};
use campfire_runner::{PlayerSlotFor, ServerInputRefusal, Session};
use campfire_values::TeamSet;
use tracing::info;

use crate::close_code::CloseCode;
use crate::net_protocol::MatchChannel;
use crate::sim_server::offering::{CheckedJoins, Dismissing, JoinLinks, Offered, Offering};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::viewer::Viewer;
use campfire_host::JoinError;
use campfire_host::Joined;
use campfire_host::LeaveMatch;
use campfire_host::MatchClock;
use campfire_host::MatchLoaded;
use campfire_host::PlayerLink;
use campfire_host::PlayerSlots;
use campfire_host::WatchStart;
use campfire_host::{ChainHead, MatchStart, PlayerSlotChain};
use campfire_host::{PlayerSlotLink, PlayerSlotLinks};

/// The session's door after the start: it offers the terms to every new link, and places each
/// player who answers. A main key that controls a player slot gets it back, its live link ended if
/// it has one, as the newest login wins; one that left a player slot still reserved, played by a
/// bot or open gets it back too; any other gets an open player slot when the mode lets a player
/// join late, else a bot's when it lets a player take a bot's over, else a refusal. It also watches
/// the placed links: a player whose link fails keeps their player slot for the grace period, and
/// leaves it after, or at once when they ask.
#[derive(Resource, Debug)]
pub(crate) struct Door {
    offering: Offering,
}

/// What the door places a player in: the player slot, and the chain they sign.
#[derive(Debug, Clone, Copy)]
struct PlayerSlotIn {
    player_slot: PlayerSlot,
    chain: PlayerSlotChain,
}

impl AsRef<Offering> for Door {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

/// What the door places a player with: the session, which logs what placing them changes, the
/// player slots, the mode, which gives a player slot's team, the match's clock, and each link's
/// sender of the match's start.
#[derive(SystemParam)]
pub(crate) struct Slotting<'w, 's> {
    commands: Commands<'w, 's>,
    session: ResMut<'w, Session>,
    player_slot_links: ResMut<'w, PlayerSlotLinks>,
    mode: Res<'w, MatchMode>,
    clock: Res<'w, MatchClock>,
    starts: Query<'w, 's, &'static mut MessageSender<MatchStart>>,
}

impl Door {
    pub(crate) const fn new(offering: Offering) -> Self {
        Self { offering }
    }

    /// Places the player of each offered link that answered, or refuses them, and admits each
    /// spectator while a place is free; the joins of one frame in `Offering::check_all`'s order.
    pub(crate) fn take_joins(
        mut door: ResMut<'_, Self>,
        mut links: JoinLinks<'_, '_>,
        viewers: Query<'_, '_, &Viewer>,
        facts: Res<'_, MatchFacts>,
        mut watch_starts: Query<'_, '_, &'static mut MessageSender<WatchStart>>,
        mut slotting: Slotting<'_, '_>,
        mut checked: Local<'_, CheckedJoins>,
    ) {
        let joins = links.iter_mut().filter_map(Offering::received);
        door.offering.check_all(joins, &mut checked);
        let watching = Spectators::count(viewers.iter());
        checked.admit(&mut DoorPlayerSlots {
            slotting: &mut slotting,
            watch_starts: &mut watch_starts,
            facts: &facts,
            watching,
        });
    }

    /// Logs what placing the player of `delegation` changes, and gives where they sit.
    fn place_player(
        slotting: &mut Slotting<'_, '_>,
        link: Entity,
        delegation: Delegation,
    ) -> Result<PlayerSlotIn, JoinError> {
        let (player_slot, chain, renews) = match slotting.session.player_slot_for(&delegation) {
            Some(PlayerSlotFor::Held {
                player_slot,
                chain,
                renews,
            }) => (player_slot, chain, renews),
            Some(PlayerSlotFor::Join(player_slot)) => {
                Self::serve(
                    &mut slotting.session,
                    ServerInput::Join {
                        player_slot,
                        delegation,
                    },
                )?;
                return Ok(Self::joined(slotting.session.log(), player_slot));
            }
            None => return Err(JoinError::NoPlayerSlot),
        };
        if renews {
            Self::serve(
                &mut slotting.session,
                ServerInput::Renew {
                    player_slot,
                    delegation,
                },
            )?;
        }
        let player_slot_link = slotting.player_slot_links.get(player_slot);
        match player_slot_link.link {
            Some(older) if older != link => {
                info!(
                    player_slot = player_slot.get(),
                    ?older,
                    "a newer login took the player's player slot"
                );
                Dismissing::refuse(&mut slotting.commands, older, JoinError::Superseded);
            }
            _ if player_slot_link.gone_since.is_some() => {
                Self::serve(
                    &mut slotting.session,
                    ServerInput::Connected { player_slot },
                )?;
            }
            _ => {}
        }
        let head = ChainHead {
            next_seq: chain.next_seq(),
            head: chain.head(),
            last_stamp: slotting.session.log().last_stamp(player_slot),
        };
        Ok(PlayerSlotIn {
            player_slot,
            chain: PlayerSlotChain::Held(head),
        })
    }

    /// Player slots each of `links` in its player slot, which the log gave its player, in its new
    /// chain, as the match starts.
    pub(super) fn player_slot_new(world: &mut World, links: &[(PlayerSlot, Entity)]) {
        let mut state = SystemState::<Slotting<'_, '_>>::new(world);
        let mut slotting = state
            .get_mut(world)
            .expect("a match that begins holds what its door places players with");
        for &(player_slot, link) in links {
            let player_slot_in = Self::joined(slotting.session.log(), player_slot);
            Self::player_slot(&mut slotting, link, player_slot_in);
        }
        state.apply(world);
    }

    /// Where the player whose join of `player_slot` `log` just took sits: in its new chain.
    fn joined(log: &SessionLog, player_slot: PlayerSlot) -> PlayerSlotIn {
        PlayerSlotIn {
            player_slot,
            chain: PlayerSlotChain::new_of(log, player_slot),
        }
    }

    /// Player slots `link` as `player_slot` says: it carries that player slot's inputs, learns the
    /// match from the next tick on, the teams' relations and any end at once, predicts the player
    /// slot's entities its kind lets its owner predict
    /// ([`KindShows`](crate::sim_server::kind_shows::KindShows)), and stands in its team's room, so
    /// it receives every unit its team sees and no other.
    fn player_slot(slotting: &mut Slotting<'_, '_>, link: Entity, player_slot_in: PlayerSlotIn) {
        let PlayerSlotIn { player_slot, chain } = player_slot_in;
        let team = slotting
            .mode
            .team_of(player_slot)
            .expect("every player slot has a team");
        slotting
            .commands
            .entity(link)
            .insert((Joined, PlayerLink::new(player_slot, team)));
        slotting.player_slot_links.set(
            player_slot,
            PlayerSlotLink {
                link: Some(link),
                gone_since: None,
            },
        );
        let log = slotting.session.log();
        let (first, loaded) = (log.next_tick(), log.is_loaded());
        slotting
            .starts
            .get_mut(link)
            .expect("a client link sends the match start")
            .send::<MatchChannel>(MatchStart {
                clock: slotting.clock.since(first),
                player_slot,
                team,
                chain,
                loaded,
            });
        info!(
            player_slot = player_slot.get(),
            link = link.to_bits(),
            "a player took their player slot"
        );
    }

    /// Takes every joined link back to the offer, as the session went back in place to a save
    /// before sim tick `tick`: each learns of the load, holds no player slot and watches nothing,
    /// so it takes no input, and the next offer goes to it as to a link that connects
    /// (08-sessions.md, A load goes back in place).
    pub(super) fn offer_again(world: &mut World, tick: Tick) {
        let mut joined =
            world.query_filtered::<(Entity, &mut MessageSender<MatchLoaded>), With<Joined>>();
        let links: Vec<Entity> = joined
            .iter_mut(world)
            .map(|(link, mut sender)| {
                sender.send::<MatchChannel>(MatchLoaded { tick });
                link
            })
            .collect();
        for link in links {
            world
                .entity_mut(link)
                .remove::<(Joined, PlayerLink, Viewer, Offered)>();
        }
    }

    /// Logs `input` into `session`, which signs it.
    fn serve(session: &mut Session, input: ServerInput<'_>) -> Result<(), JoinError> {
        session.serve(input).map_err(JoinError::Refused)
    }

    /// Watches the placed links: a link that failed logs its player slot `Disconnected`, and the
    /// grace period runs from then by the server's real clock; a player slot whose player is still
    /// gone after it logs the player's `Leave` after grace; a player who asks to leave logs `Leave`
    /// at once, and their link ends.
    pub(crate) fn watch(
        clock: Res<'_, Clock>,
        door: Res<'_, Self>,
        mut session: ResMut<'_, Session>,
        mut player_slot_links: ResMut<'_, PlayerSlotLinks>,
        mut leaving: Query<'_, '_, &mut MessageReceiver<LeaveMatch>>,
        connected: Query<'_, '_, (), With<Connected>>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = clock.now().elapsed();
        let grace = door.offering.times.grace;
        let player_slots = session.log().player_slot_count();
        for player_slot in (0..player_slots).map(PlayerSlot::new) {
            let player_slot_link = player_slot_links.get(player_slot);
            if let Some(link) = player_slot_link.link {
                let asked = leaving
                    .get_mut(link)
                    .is_ok_and(|mut receiver| receiver.receive().count() > 0);
                if asked {
                    Self::leave(&mut session, player_slot, LeaveReason::Asked);
                    player_slot_links.set(player_slot, PlayerSlotLink::default());
                    if let Ok(mut gone) = commands.get_entity(link) {
                        gone.try_remove::<PlayerLink>();
                    }
                    commands.trigger(Disconnect {
                        entity: link,
                        reason: CloseCode::Left.app(),
                    });
                    continue;
                }
                if connected.contains(link) {
                    continue;
                }
                info!(
                    player_slot = player_slot.get(),
                    "a player's link failed; their grace period runs"
                );
                Self::serve_unless_full(&mut session, ServerInput::Disconnected { player_slot });
                if let Ok(mut gone) = commands.get_entity(link) {
                    gone.try_remove::<PlayerLink>();
                }
                let gone = PlayerSlotLink {
                    link: None,
                    gone_since: Some(now),
                };
                player_slot_links.set(player_slot, gone);
            } else if let Some(since) = player_slot_link.gone_since
                && now.saturating_sub(since) >= grace
            {
                info!(
                    player_slot = player_slot.get(),
                    "a player's grace period passed"
                );
                let player = matches!(
                    session.log().controller(player_slot),
                    Some(Controller::Player { .. })
                );
                if player {
                    Self::leave(&mut session, player_slot, LeaveReason::Grace);
                }
                player_slot_links.set(player_slot, PlayerSlotLink::default());
            }
        }
    }

    /// Logs the leave of `player_slot`'s player for `reason`, the player slot becoming what the
    /// mode says.
    fn leave(session: &mut Session, player_slot: PlayerSlot, reason: LeaveReason) {
        let becomes = session.after_leave();
        info!(
            player_slot = player_slot.get(),
            ?reason,
            ?becomes,
            "a player left"
        );
        Self::serve_unless_full(
            session,
            ServerInput::Leave {
                player_slot,
                reason,
                becomes,
            },
        );
    }

    /// Logs `input`, a change of a link or a leave, which its player slot's player always allows,
    /// into `session`, which signs it; none when the log is full, as the session then ends at
    /// the next boundary, before the tick it would apply in.
    pub(crate) fn serve_unless_full(session: &mut Session, input: ServerInput<'_>) {
        match Self::serve(session, input) {
            Ok(())
            | Err(JoinError::Refused(ServerInputRefusal::Log(ServerInputError::LogFull))) => {}
            Err(error) => panic!("the server's own input holds: {}", ErrorReport::of(&error)),
        }
    }

    /// Whether every player slot a player controlled has left, and no player holds a link: the
    /// session then ends.
    pub(super) fn is_empty(world: &World) -> bool {
        let none_held = world
            .resource::<Session>()
            .log()
            .played_player_slots()
            .next()
            .is_none();
        none_held
            && world
                .resource::<PlayerSlotLinks>()
                .iter()
                .all(|(_, player_slot_link)| player_slot_link.link.is_none())
    }
}

/// The door's player slots for a frame's joins, with the spectators already watching.
struct DoorPlayerSlots<'a, 'w, 's, 'qw, 'qs> {
    slotting: &'a mut Slotting<'w, 's>,
    watch_starts: &'a mut Query<'qw, 'qs, &'static mut MessageSender<WatchStart>>,
    facts: &'a MatchFacts,
    watching: usize,
}

/// The player slots by their count of spectators; the system params they borrow show nothing more.
impl fmt::Debug for DoorPlayerSlots<'_, '_, '_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DoorPlayerSlots")
            .field("watching", &self.watching)
            .finish_non_exhaustive()
    }
}

impl<'w, 's> PlayerSlots<'w, 's> for DoorPlayerSlots<'_, 'w, 's, '_, '_> {
    fn commands(&mut self) -> &mut Commands<'w, 's> {
        &mut self.slotting.commands
    }

    /// Admits the spectator, and tells it the match's clock from the next tick on.
    fn watch(&mut self, link: Entity, teams: TeamSet) -> Result<(), JoinError> {
        let viewer = Spectators::place(&self.facts.visibility, self.watching, teams)?;
        self.slotting.commands.entity(link).insert((Joined, viewer));
        self.watching += 1;
        let first = self.slotting.session.log().next_tick();
        let mut sender = self
            .watch_starts
            .get_mut(link)
            .expect("a client link sends the watch start");
        Spectators::tell_start(&mut sender, &self.slotting.clock, first, link);
        Ok(())
    }

    fn player_slot(&mut self, link: Entity, delegation: Delegation) -> Result<(), JoinError> {
        let player_slot_in = Door::place_player(self.slotting, link, delegation)?;
        Door::player_slot(self.slotting, link, player_slot_in);
        Ok(())
    }
}
