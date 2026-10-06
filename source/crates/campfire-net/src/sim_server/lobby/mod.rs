use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use campfire_package::ModePackages;
use campfire_protocol::{
    ConnectChallenge, Delegation, SeedChain, SessionHeader, SessionLog, SessionTerms, SlotPlan,
    SlotStart,
};
use campfire_runner::{InputRules, SessionRules};
use lightyear::prelude::{Connected, LocalTimeline};
use tracing::info;

use crate::error::{JoinError, LobbyError};
use crate::events::join_refused::JoinRefused;
use crate::join::Join;
use crate::sim_server::offering::{JoinLinks, Joined, OfferLinks, Offering, Refused, Superseding};
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::SessionFiles;
use crate::sim_server::{SessionStart, SimServer};

/// A session open for players to join: the server offers each connected client the terms and a
/// challenge, checks each answer, gives the players slots in the order they joined, and starts
/// the match when every slot is taken. A main key that joined again takes its seat back, and its
/// older link ends.
#[derive(Resource, Debug)]
pub struct Lobby {
    offering: Offering,
    seed_chain: SeedChain,
    packages: Arc<ModePackages>,
    /// How many slots players take: every slot but the bots'.
    players: usize,
    bots: ServerBots,
    setup: ServerSetup,
    /// The links that joined, by slot, with their delegations.
    joined: Vec<(Entity, Delegation)>,
    /// Where the session's log and snapshots go as the server writes them, once it keeps them.
    files: Option<SessionFiles>,
}

/// What a server opens a session with.
#[derive(Debug)]
pub struct LobbySetup {
    pub packages: Arc<ModePackages>,
    pub seed_chain: SeedChain,
    /// Ticks a second, which the mode's range must hold.
    pub tick_hz: NonZeroU32,
    pub inputs: InputRules,
    /// How many slots the session plays.
    pub slots: usize,
    /// The bots the server plays: their slots are the bots'.
    pub bots: ServerBots,
    /// The slots no one takes at the start, which a late join may take; the rest but the bots'
    /// are the players'.
    pub open: Vec<PlayerSlot>,
    pub server: ServerSetup,
}

impl Lobby {
    /// A session of the mode `packages` holds, by the setup's rules; an error when the mode does
    /// not run at the setup's tick rate or has not as many slots, when a bot or an open slot
    /// names a slot the session does not have or one already named, and when no slot is left to
    /// a player.
    pub fn new(setup: LobbySetup) -> Result<Lobby, LobbyError> {
        let LobbySetup {
            packages,
            seed_chain,
            tick_hz,
            inputs,
            slots,
            bots,
            open,
            server,
        } = setup;
        let mut plan = vec![SlotPlan::Player; slots];
        let named = (bots.slots.iter().map(|bot| (bot.slot, SlotPlan::Bot)))
            .chain(open.into_iter().map(|slot| (slot, SlotPlan::Open)));
        for (slot, kind) in named {
            let at = plan
                .get_mut(slot.index())
                .ok_or(LobbyError::NoSuchSlot(slot))?;
            if *at != SlotPlan::Player {
                return Err(LobbyError::SlotNamedTwice(slot));
            }
            *at = kind;
        }
        let players = plan
            .iter()
            .filter(|&&plan| plan == SlotPlan::Player)
            .count();
        if players == 0 {
            return Err(LobbyError::NoPlayer);
        }
        let terms = SessionRules::of(&packages)
            .terms(
                server.key.x_only_public_key().0,
                seed_chain.commitment(),
                tick_hz,
                inputs,
                plan,
            )
            .map_err(LobbyError::Terms)?;
        Ok(Lobby {
            offering: Offering::new(terms, &server),
            seed_chain,
            packages,
            players,
            bots,
            setup: server,
            joined: Vec::with_capacity(players),
            files: None,
        })
    }

    /// Keeps `files`, a new journal and where snapshots go, for the session: the journal follows
    /// the log from the match's start.
    pub fn keep_files(&mut self, files: SessionFiles) {
        assert!(self.files.is_none(), "a session keeps one journal");
        self.files = Some(files);
    }

    pub const fn terms(&self) -> &SessionTerms {
        &self.offering.terms
    }

    /// How many players joined so far.
    const fn joined(&self) -> usize {
        self.joined.len()
    }

    /// Offers the terms to each connected link that has no offer; see `Offering::offer`.
    pub(crate) fn offer(
        lobby: Res<'_, Lobby>,
        timeline: Res<'_, LocalTimeline>,
        mut links: OfferLinks<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        lobby
            .offering
            .offer(timeline.tick(), &mut links, &mut commands);
    }

    /// Takes each offered link's join: a player who answered their challenge joins in the next
    /// free slot, or the seat their main key held, whose older link ends; a refused link keeps
    /// its reason.
    pub(crate) fn take_joins(
        mut lobby: ResMut<'_, Lobby>,
        mut links: JoinLinks<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        for (link, offered, mut receiver) in &mut links {
            let Some(join) = receiver.receive().next() else {
                continue;
            };
            match lobby.take(link, offered.challenge, &join) {
                Ok(older) => {
                    info!(
                        ?link,
                        joined = lobby.joined(),
                        of = lobby.players,
                        "a player joined"
                    );
                    commands.entity(link).insert(Joined);
                    if let Some(older) = older {
                        info!(?older, "a newer login took the player's seat");
                        Superseding::mark(&mut commands, older);
                    }
                }
                Err(error) => {
                    JoinRefused {
                        link: format!("{link:?}"),
                        error: error.to_string(),
                    }
                    .log();
                    commands.entity(link).insert(Refused(error));
                }
            }
        }
    }

    /// Starts the match once every slot is taken, and closes the lobby: the door takes the
    /// joins from then on.
    pub(crate) fn start_when_full(world: &mut World) {
        if world.resource::<Lobby>().joined.len() < world.resource::<Lobby>().players {
            return;
        }
        let lobby = world.remove_resource::<Lobby>().expect("the lobby is open");
        let terms = lobby.offering.terms.clone();
        let mut joined = lobby.joined.into_iter();
        let mut links = Vec::with_capacity(lobby.players);
        let mut slots = Vec::with_capacity(terms.slots.len());
        for (slot, plan) in (0..).map(PlayerSlot::new).zip(&terms.slots) {
            slots.push(match plan {
                SlotPlan::Player => {
                    let (link, delegation) = joined.next().expect("a player for each slot");
                    links.push((slot, link));
                    SlotStart::player(delegation)
                }
                SlotPlan::Bot => SlotStart::Bot,
                SlotPlan::Open => SlotStart::Open,
            });
        }
        let header = SessionHeader { terms, slots };
        let session = header.terms.session_id();
        let log = SessionLog::new(header).expect("every delegation names this session");
        info!(%session, players = links.len(), "every slot is taken; the match starts");
        let seeds = lobby.seed_chain.seeds();
        let start = SessionStart {
            log,
            seeds,
            packages: &lobby.packages,
            files: lobby.files,
            server: &lobby.setup,
            bots: lobby.bots,
        };
        SimServer::start_match(world, start, &links)
            .expect("the lobby's terms come from its own packages");
    }

    /// Frees the seat of each joined link that is no longer connected, so a match starts only
    /// with live links, and the seat goes to the next player who joins.
    pub(crate) fn free_seats(
        mut lobby: ResMut<'_, Lobby>,
        live: Query<'_, '_, (), (With<Connected>, With<Joined>)>,
        mut commands: Commands<'_, '_>,
    ) {
        lobby.joined.retain(|&(link, _)| {
            let connected = live.contains(link);
            if !connected {
                info!(?link, "a player left the lobby; their seat is free");
                if let Ok(mut gone) = commands.get_entity(link) {
                    gone.remove::<Joined>();
                }
            }
            connected
        });
    }

    /// Seats the player of `link` if their `join` answers `challenge`: in the seat their main key
    /// holds, whose older link it gives, or else in the next free slot.
    fn take(
        &mut self,
        link: Entity,
        challenge: ConnectChallenge,
        join: &Join,
    ) -> Result<Option<Entity>, JoinError> {
        let delegation = self.offering.check(challenge, join)?;
        let main_key = delegation.main_key();
        if let Some(seat) = self
            .joined
            .iter_mut()
            .find(|(_, seated)| seated.main_key() == main_key)
        {
            let older = seat.0;
            *seat = (link, delegation);
            return Ok(Some(older));
        }
        if self.joined.len() == self.players {
            return Err(JoinError::Full);
        }
        self.joined.push((link, delegation));
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
