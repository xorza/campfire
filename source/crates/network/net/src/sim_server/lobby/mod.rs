use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, ResMut};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_host::JoinError;
use campfire_host::Joined;
use campfire_host::PlayerSlots;
use campfire_host::ServerBots;
use campfire_host::ServerDir;
use campfire_host::ServerPhase;
use campfire_host::ServerSetup;
use campfire_host::{SessionDir, SessionFiles};
use campfire_mode::{HostPicks, OptionPicks, OptionValue, PlayerSlotPick, PlayerSlotPicks};
use campfire_net_connection::Connected;
use campfire_package::ModePackages;
use campfire_protocol::{
    Delegation, InputRules, PlayerSlotPlan, PlayerSlotStart, SeedChain, SessionHeader, SessionLog,
    SessionPrivate, SessionTerms,
};
use campfire_runner::{CheckedTerms, HostChoices, OfferedTerms, SessionRules};
use campfire_values::TeamSet;
use campfire_visibility::HostVisibility;
use tracing::info;

use crate::sim_server::lobby::error::{KeepFilesError, LobbyError, LobbyOpenError};
use crate::sim_server::offering::{CheckedJoins, Dismissing, JoinLinks, Offering};
use crate::sim_server::server_match::{MatchBegin, MatchSource, NewMatch, ServerMatch};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::spectators::spectators_world::SpectatorsWorld;
use crate::sim_server::viewer::Viewer;

pub(crate) mod error;

/// A session open for players to join: the server offers each connected client the terms and a
/// challenge, checks each answer, gives the players player slots in the order they joined, those of
/// one frame in `Offering::check_all`'s order, and starts the match when every player slot is
/// taken. A main key that joined again takes its player slot back, and its older link ends.
#[derive(Resource, Debug)]
pub struct Lobby {
    offering: Offering,
    seed_chain: SeedChain,
    packages: Arc<ModePackages>,
    /// How many player slots players take: every player slot but the bots'.
    players: usize,
    /// Whom it waits for before its match starts.
    start: LobbyStart,
    bots: ServerBots,
    server: ServerSetup,
    /// The links that joined, by player slot, with their delegations.
    joined: Vec<(Entity, Delegation)>,
    /// The session's terms as the mode's rules made them: its visibility, which bounds its
    /// spectators, and its picks.
    checked: CheckedTerms,
    /// Where the session's log and snapshots go as the server writes them, once it keeps them.
    files: Option<SessionFiles>,
}

/// What a server opens a new session of its own with, which `Lobby::open` makes the rest of.
#[derive(Debug)]
pub struct LobbyOpening {
    pub packages: Arc<ModePackages>,
    /// The bots the server plays: their player slots are the bots'.
    pub bots: ServerBots,
    /// The player slots no one takes at the start, which a late join may take; the rest but the
    /// bots' are the players'.
    pub open: Vec<PlayerSlot>,
    /// What the host sets of the session's visibility, within what the mode leaves it.
    pub host: HostVisibility,
    /// The values the host picks of the mode's options, the others their defaults.
    pub options: Vec<OptionValue>,
    /// The values the host picks of each player slot's choices of values, the others none.
    pub player_slot_picks: Vec<PlayerSlotPick>,
    pub server: ServerSetup,
    pub start: LobbyStart,
}

/// Whom a lobby waits for before its match starts: every player slot taken, of a plan of one
/// player slot at least; or, of a local watch, also its watcher, the client that hosts it, of a
/// plan that may hold bots alone (08-sessions.md, A local session may hold bots alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LobbyStart {
    Players,
    Watched,
}

/// What a server opens a session with: its opening, and the session's rules a new session of
/// its own takes from the mode and the server's entropy.
#[derive(Debug)]
pub struct LobbySetup {
    pub opening: LobbyOpening,
    pub seed_chain: SeedChain,
    /// Ticks a second, which the mode's range must hold.
    pub tick_hz: NonZeroU32,
    pub inputs: InputRules,
    /// How many player slots the session plays.
    pub player_slots: usize,
}

impl AsRef<Offering> for Lobby {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

impl Lobby {
    /// A new session of a server, as `new` opens it, its files kept under `data`: every player_slot
    /// the mode has, its seed chain fresh from the server's entropy, at the mode's default rate,
    /// its inputs within `InputRules::LAN`. An error when it does not open or keeps no files.
    pub fn open(opening: LobbyOpening, data: &ServerDir) -> Result<Self, LobbyOpenError> {
        let root = opening.server.os.entropy.random();
        let manifest = opening.packages.manifest();
        let player_slots =
            usize::try_from(manifest.player_slots()).expect("player slots fit usize");
        let tick_hz = manifest.tick_hz.default();
        let mut lobby = Self::new(LobbySetup {
            opening,
            seed_chain: SeedChain::new(root, SeedChain::MAX_LEN),
            tick_hz,
            inputs: InputRules::LAN,
            player_slots,
        })?;
        lobby.keep_files(data)?;
        Ok(lobby)
    }

    /// A session of the mode `packages` holds, by the setup's rules; an error when the mode does
    /// not run at the setup's tick rate or has not as many player slots, when a bot or an open
    /// player_slot names a player slot the session does not have or one already named, when no
    /// player slot is left to a player, when the host sets a visibility the mode does not leave it,
    /// when it picks a value of no option of the mode's, and when it picks a player slot's value no
    /// choice of the mode's lists.
    pub fn new(setup: LobbySetup) -> Result<Self, LobbyError> {
        let LobbySetup {
            opening:
                LobbyOpening {
                    packages,
                    bots,
                    open,
                    host,
                    options,
                    player_slot_picks,
                    server,
                    start,
                },
            seed_chain,
            tick_hz,
            inputs,
            player_slots,
        } = setup;
        let mut plan = vec![PlayerSlotPlan::Player; player_slots];
        let named = (bots
            .player_slots
            .iter()
            .map(|bot| (bot.player_slot, PlayerSlotPlan::Bot)))
        .chain(
            open.into_iter()
                .map(|player_slot| (player_slot, PlayerSlotPlan::Open)),
        );
        for (player_slot, kind) in named {
            let at = plan
                .get_mut(player_slot.index())
                .ok_or(LobbyError::NoSuchPlayerSlot(player_slot))?;
            if *at != PlayerSlotPlan::Player {
                return Err(LobbyError::PlayerSlotNamedTwice(player_slot));
            }
            *at = kind;
        }
        let players = plan
            .iter()
            .filter(|&&plan| plan == PlayerSlotPlan::Player)
            .count();
        if players == 0 && start == LobbyStart::Players {
            return Err(LobbyError::NoPlayer);
        }
        let session_rules = SessionRules::of(&packages);
        let visibility = session_rules
            .visibility()
            .session(&host)
            .map_err(LobbyError::Visibility)?;
        let options =
            OptionPicks::named(session_rules.options(), &options).map_err(LobbyError::Options)?;
        let choices = &packages.data().choices;
        let player_slot_picks = PlayerSlotPicks::named(choices, player_slots, &player_slot_picks)
            .map_err(LobbyError::PlayerSlotPicks)?;
        let picks = HostPicks {
            options,
            player_slots: player_slot_picks,
        };
        let OfferedTerms { terms, checked } = session_rules
            .terms(HostChoices {
                server_key: server.signer.public_key(),
                seed_commitment: seed_chain.commitment(),
                tick_hz,
                inputs,
                player_slots: plan,
                visibility: &visibility,
                picks: &picks,
            })
            .map_err(LobbyError::Terms)?;
        Ok(Self {
            offering: Offering::new(terms, &server),
            seed_chain,
            packages,
            players,
            start,
            bots,
            server,
            joined: Vec::with_capacity(players),
            checked,
            files: None,
        })
    }

    /// Makes the session's directory under `data`, with its private record, a new journal and
    /// where snapshots go, and keeps its files: the journal follows the log from the match's
    /// start.
    pub fn keep_files(&mut self, data: &ServerDir) -> Result<(), KeepFilesError> {
        assert!(self.files.is_none(), "a session keeps one journal");
        let private = SessionPrivate {
            seed_chain: self.seed_chain.clone(),
        };
        let session = self.terms().session_id();
        let dir = SessionDir::create(data, session, &private).map_err(KeepFilesError::Dir)?;
        self.files = Some(dir.start().map_err(KeepFilesError::Journal)?);
        Ok(())
    }

    /// The session's terms as the mode's rules made them.
    pub const fn checked(&self) -> &CheckedTerms {
        &self.checked
    }

    pub fn terms(&self) -> &SessionTerms {
        &self.offering.terms
    }

    /// The packages of the session's mode.
    pub const fn packages(&self) -> &Arc<ModePackages> {
        &self.packages
    }

    /// The setup of the server that opened it.
    pub const fn server(&self) -> &ServerSetup {
        &self.server
    }

    /// How many players joined so far.
    const fn joined(&self) -> usize {
        self.joined.len()
    }

    /// Takes each offered link's join: a player who answered their challenge joins in the next free
    /// player slot, or the player slot their main key held, whose older link ends; one who answered
    /// to watch takes a spectator's place while one is free; a refused link keeps its reason. The
    /// joins of one frame take their player slots in `Offering::check_all`'s order.
    pub(crate) fn take_joins(
        mut lobby: ResMut<'_, Self>,
        mut links: JoinLinks<'_, '_>,
        viewers: Query<'_, '_, &Viewer>,
        mut commands: Commands<'_, '_>,
        mut checked: Local<'_, CheckedJoins>,
    ) {
        let joins = links.iter_mut().filter_map(Offering::received);
        lobby.offering.check_all(joins, &mut checked);
        let watching = Spectators::count(viewers.iter());
        checked.admit(&mut LobbyPlayerSlots {
            lobby: &mut lobby,
            commands: &mut commands,
            watching,
        });
    }

    /// Starts the match once every player slot is taken and, of a watched lobby, its watcher
    /// joined, and closes the lobby: the door takes the joins from then on.
    pub(crate) fn start_when_full(world: &mut World) {
        let lobby = world.resource::<Self>();
        if lobby.joined.len() < lobby.players {
            return;
        }
        if lobby.start == LobbyStart::Watched {
            let mut viewers = world.query::<&Viewer>();
            if Spectators::count(viewers.iter(world)) == 0 {
                return;
            }
        }
        let lobby = world.remove_resource::<Self>().expect("the lobby is open");
        let terms = SessionTerms::clone(&lobby.offering.terms);
        let mut joined = lobby.joined.into_iter();
        let mut links = Vec::with_capacity(lobby.players);
        let mut player_slots = Vec::with_capacity(terms.player_slots.len());
        for (player_slot, plan) in (0..).map(PlayerSlot::new).zip(&terms.player_slots) {
            player_slots.push(match plan {
                PlayerSlotPlan::Player => {
                    let (link, delegation) = joined.next().expect("a player for each player slot");
                    links.push((player_slot, link));
                    PlayerSlotStart::player(delegation)
                }
                PlayerSlotPlan::Bot => PlayerSlotStart::Bot,
                PlayerSlotPlan::Open => PlayerSlotStart::Open,
            });
        }
        let header = SessionHeader {
            terms,
            player_slots,
        };
        let session = header.terms.session_id();
        let log = SessionLog::new(header)
            .expect("every delegation names this session, one main key each");
        info!(%session, players = links.len(), "every player slot is taken; the match starts");
        let seeds = lobby.seed_chain.seeds();
        let begin = MatchBegin {
            source: MatchSource::New(NewMatch {
                log,
                seeds,
                files: lobby.files,
                clients: &links,
            }),
            packages: &lobby.packages,
            server: &lobby.server,
            bots: lobby.bots,
            checked: lobby.checked,
        };
        ServerMatch::begin(world, begin).expect("the lobby's terms come from its own packages");
        ServerPhase::Running.go(world);
        world.start_spectators();
    }

    /// Frees the player slot of each joined link that is no longer connected, so a match starts
    /// only with live links, and the player slot goes to the next player who joins.
    pub(crate) fn free_player_slots(
        mut lobby: ResMut<'_, Self>,
        live: Query<'_, '_, (), (With<Connected>, With<Joined>)>,
        mut commands: Commands<'_, '_>,
    ) {
        lobby.joined.retain(|&(link, _)| {
            let connected = live.contains(link);
            if !connected {
                info!(
                    link = link.to_bits(),
                    "a player left the lobby; their player slot is free"
                );
                if let Ok(mut gone) = commands.get_entity(link) {
                    gone.remove::<Joined>();
                }
            }
            connected
        });
    }

    /// Places the player of `link`, whose `delegation` the offering checked: in the player slot
    /// their main key holds, whose older link it gives, or else in the next free player slot.
    fn player_slot(
        &mut self,
        link: Entity,
        delegation: Delegation,
    ) -> Result<Option<Entity>, JoinError> {
        let main_key = delegation.main_key();
        if let Some(joined) = self
            .joined
            .iter_mut()
            .find(|(_, held)| held.main_key() == main_key)
        {
            let older = joined.0;
            *joined = (link, delegation);
            return Ok(Some(older));
        }
        if self.joined.len() == self.players {
            return Err(JoinError::Full);
        }
        self.joined.push((link, delegation));
        Ok(None)
    }
}

/// The lobby's player slots for a frame's joins, with the spectators already watching.
struct LobbyPlayerSlots<'a, 'w, 's> {
    lobby: &'a mut Lobby,
    commands: &'a mut Commands<'w, 's>,
    watching: usize,
}

/// The player slots by their lobby and their count of spectators; the commands show nothing.
impl fmt::Debug for LobbyPlayerSlots<'_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LobbyPlayerSlots")
            .field("lobby", &self.lobby)
            .field("watching", &self.watching)
            .finish_non_exhaustive()
    }
}

impl<'w, 's> PlayerSlots<'w, 's> for LobbyPlayerSlots<'_, 'w, 's> {
    fn commands(&mut self) -> &mut Commands<'w, 's> {
        self.commands
    }

    fn watch(&mut self, link: Entity, teams: TeamSet) -> Result<(), JoinError> {
        let viewer = Spectators::place(&self.lobby.checked.visibility, self.watching, teams)?;
        self.commands.entity(link).insert((Joined, viewer));
        self.watching += 1;
        info!(link = link.to_bits(), "a spectator joined");
        Ok(())
    }

    fn player_slot(&mut self, link: Entity, delegation: Delegation) -> Result<(), JoinError> {
        let older = self.lobby.player_slot(link, delegation)?;
        info!(
            link = link.to_bits(),
            joined = self.lobby.joined(),
            of = self.lobby.players,
            "a player joined"
        );
        self.commands.entity(link).insert(Joined);
        if let Some(older) = older {
            info!(?older, "a newer login took the player's player slot");
            Dismissing::refuse(self.commands, older, JoinError::Superseded);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
