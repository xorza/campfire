use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, ResMut};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_package::ModePackages;
use campfire_protocol::{
    Delegation, InputRules, SeedChain, SessionHeader, SessionLog, SessionPrivate, SessionTerms,
    SlotPlan, SlotStart,
};
use campfire_runner::{HostChoices, SessionRules};
use campfire_visibility::{HostVisibility, Visibility};
use lightyear::prelude::Connected;
use tracing::info;

use crate::sim_server::error::JoinError;
use crate::sim_server::lobby::error::{KeepFilesError, LobbyError, LobbyOpenError};
use crate::sim_server::offering::{CheckedJoins, Dismissing, JoinLinks, Joined, Offering};
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::{SessionDir, SessionFiles};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::spectators::spectators_world::SpectatorsWorldCrate;
use crate::sim_server::viewer::Viewer;
use crate::sim_server::{SessionStart, SimServer};

pub(crate) mod error;

/// A session open for players to join: the server offers each connected client the terms and a
/// challenge, checks each answer, gives the players slots in the order they joined, those of one
/// frame in `Offering::check_all`'s order, and starts the match when every slot is taken. A main key that joined again takes its seat back, and its
/// older link ends.
#[derive(Resource, Debug)]
pub struct Lobby {
    offering: Offering,
    seed_chain: SeedChain,
    packages: Arc<ModePackages>,
    /// How many slots players take: every slot but the bots'.
    players: usize,
    bots: ServerBots,
    server: ServerSetup,
    /// The links that joined, by slot, with their delegations.
    joined: Vec<(Entity, Delegation)>,
    /// The session's visibility, which bounds its spectators.
    visibility: Visibility,
    /// Where the session's log and snapshots go as the server writes them, once it keeps them.
    files: Option<SessionFiles>,
}

/// What a server opens a new session of its own with, which `Lobby::open` makes the rest of.
#[derive(Debug)]
pub struct LobbyOpening {
    pub packages: Arc<ModePackages>,
    /// The bots the server plays: their slots are the bots'.
    pub bots: ServerBots,
    /// The slots no one takes at the start, which a late join may take; the rest but the bots'
    /// are the players'.
    pub open: Vec<PlayerSlot>,
    /// What the host sets of the session's visibility, within what the mode leaves it.
    pub host: HostVisibility,
    pub server: ServerSetup,
}

/// What a server opens a session with.
#[derive(Debug)]
pub struct LobbySetup {
    pub packages: Arc<ModePackages>,
    pub seed_chain: SeedChain,
    /// Ticks a second, which the mode's range must hold.
    pub tick_hz: NonZeroU32,
    pub rules: InputRules,
    /// How many slots the session plays.
    pub slots: usize,
    /// The bots the server plays: their slots are the bots'.
    pub bots: ServerBots,
    /// The slots no one takes at the start, which a late join may take; the rest but the bots'
    /// are the players'.
    pub open: Vec<PlayerSlot>,
    /// What the host sets of the session's visibility, within what the mode leaves it.
    pub host: HostVisibility,
    pub server: ServerSetup,
}

impl AsRef<Offering> for Lobby {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

impl Lobby {
    /// The segments of a server's seed chain: the most checkpoints a session may take, less one.
    const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();

    /// A new session of a server, as `new` opens it, its files kept under `data`: every slot
    /// the mode has, its seed chain fresh from the server's entropy, at the mode's default rate,
    /// its inputs within `InputRules::LAN`. An error when it does not open or keeps no files.
    pub fn open(opening: LobbyOpening, data: &ServerDir) -> Result<Lobby, LobbyOpenError> {
        let LobbyOpening {
            packages,
            bots,
            open,
            host,
            server,
        } = opening;
        let mut root = [0; 32];
        server.os.entropy.fill(&mut root);
        let manifest = packages.manifest();
        let slots = usize::try_from(manifest.slots()).expect("slots fit usize");
        let tick_hz = manifest.tick_hz.default();
        let mut lobby = Lobby::new(LobbySetup {
            packages,
            seed_chain: SeedChain::new(root, Lobby::SEGMENTS),
            tick_hz,
            rules: InputRules::LAN,
            slots,
            bots,
            open,
            host,
            server,
        })?;
        lobby.keep_files(data)?;
        Ok(lobby)
    }

    /// A session of the mode `packages` holds, by the setup's rules; an error when the mode does
    /// not run at the setup's tick rate or has not as many slots, when a bot or an open slot
    /// names a slot the session does not have or one already named, when no slot is left to a
    /// player, and when the host sets a visibility the mode does not leave it.
    pub fn new(setup: LobbySetup) -> Result<Lobby, LobbyError> {
        let LobbySetup {
            packages,
            seed_chain,
            tick_hz,
            rules,
            slots,
            bots,
            open,
            host,
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
        let session_rules = SessionRules::of(&packages);
        let visibility = session_rules
            .visibility()
            .session(&host)
            .map_err(LobbyError::Visibility)?;
        let terms = session_rules
            .terms(HostChoices {
                server_key: server.key.x_only_public_key().0,
                seed_commitment: seed_chain.commitment(),
                tick_hz,
                inputs: rules,
                slots: plan,
                visibility: &visibility,
            })
            .map_err(LobbyError::Terms)?;
        Ok(Lobby {
            offering: Offering::new(terms, &server),
            seed_chain,
            packages,
            players,
            bots,
            server,
            joined: Vec::with_capacity(players),
            visibility,
            files: None,
        })
    }

    /// Makes the session's directory under `data`, with its private record, a new journal and
    /// where snapshots go, and keeps its files: the journal follows the log from the match's
    /// start.
    pub fn keep_files(&mut self, data: &ServerDir) -> Result<(), KeepFilesError> {
        assert!(self.files.is_none(), "a session keeps one journal");
        let private = SessionPrivate {
            seed_chain: self.seed_chain,
            terms: self.terms().clone(),
        };
        let dir = SessionDir::create(data, &private).map_err(KeepFilesError::Dir)?;
        self.files = Some(dir.start().map_err(KeepFilesError::Journal)?);
        Ok(())
    }

    pub const fn terms(&self) -> &SessionTerms {
        &self.offering.terms
    }

    /// The setup of the server that opened it.
    pub const fn server(&self) -> &ServerSetup {
        &self.server
    }

    /// How many players joined so far.
    const fn joined(&self) -> usize {
        self.joined.len()
    }

    /// Takes each offered link's join: a player who answered their challenge joins in the next
    /// free slot, or the seat their main key held, whose older link ends; one who answered to
    /// watch takes a spectator's place while one is free; a refused link keeps its reason. The
    /// joins of one frame take their slots in `Offering::check_all`'s order.
    pub(crate) fn take_joins(
        mut lobby: ResMut<'_, Lobby>,
        mut links: JoinLinks<'_, '_>,
        viewers: Query<'_, '_, &Viewer>,
        mut commands: Commands<'_, '_>,
        mut checked: Local<'_, CheckedJoins>,
    ) {
        let joins = links.iter_mut().filter_map(Offering::received);
        lobby.offering.check_all(joins, &mut checked);
        for (link, error) in checked.refused.drain(..) {
            Offering::refuse(&mut commands, link, error);
        }
        let mut watching = Spectators::count(viewers.iter());
        for link in checked.watching.drain(..) {
            match Spectators::place(&lobby.visibility, watching) {
                Ok(viewer) => {
                    commands.entity(link).insert((Joined, viewer));
                    watching += 1;
                    info!(?link, "a spectator joined");
                }
                Err(error) => Offering::refuse(&mut commands, link, error),
            }
        }
        for (link, delegation) in checked.answered.drain(..) {
            match lobby.seat(link, delegation) {
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
                        Dismissing::refuse(&mut commands, older, JoinError::Superseded);
                    }
                }
                Err(error) => Offering::refuse(&mut commands, link, error),
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
            server: &lobby.server,
            bots: lobby.bots,
            offering: lobby.offering,
        };
        SimServer::start_match(world, start, &links)
            .expect("the lobby's terms come from its own packages");
        world.start_spectators();
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

    /// Seats the player of `link`, whose `delegation` the offering checked: in the seat their main
    /// key holds, whose older link it gives, or else in the next free slot.
    fn seat(&mut self, link: Entity, delegation: Delegation) -> Result<Option<Entity>, JoinError> {
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
