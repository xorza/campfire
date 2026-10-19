use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, ResMut};
use bevy_ecs::world::World;
use campfire_common::Seat;
use campfire_mode::{OptionPicks, OptionValue};
use campfire_package::ModePackages;
use campfire_protocol::{
    Delegation, InputRules, SeatPlan, SeatStart, SeedChain, SessionHeader, SessionLog,
    SessionPrivate, SessionTerms,
};
use campfire_runner::{CheckedTerms, HostChoices, SessionRules};
use campfire_visibility::HostVisibility;
use lightyear::prelude::Connected;
use tracing::info;

use crate::error::JoinError;
use crate::sim_server::lobby::error::{KeepFilesError, LobbyError, LobbyOpenError};
use crate::sim_server::offering::{CheckedJoins, Dismissing, JoinLinks, Joined, Offering};
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_match::{MatchBegin, MatchSource, NewMatch, ServerMatch};
use crate::sim_server::server_phase::ServerPhase;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::{SessionDir, SessionFiles};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::spectators::spectators_world::SpectatorsWorld;
use crate::sim_server::viewer::Viewer;

pub(crate) mod error;

/// A session open for players to join: the server offers each connected client the terms and a
/// challenge, checks each answer, gives the players seats in the order they joined, those of one
/// frame in `Offering::check_all`'s order, and starts the match when every seat is taken. A main
/// key that joined again takes its seat back, and its older link ends.
#[derive(Resource, Debug)]
pub struct Lobby {
    offering: Offering,
    seed_chain: SeedChain,
    packages: Arc<ModePackages>,
    /// How many seats players take: every seat but the bots'.
    players: usize,
    bots: ServerBots,
    server: ServerSetup,
    /// The links that joined, by seat, with their delegations.
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
    /// The bots the server plays: their seats are the bots'.
    pub bots: ServerBots,
    /// The seats no one takes at the start, which a late join may take; the rest but the bots'
    /// are the players'.
    pub open: Vec<Seat>,
    /// What the host sets of the session's visibility, within what the mode leaves it.
    pub host: HostVisibility,
    /// The values the host picks of the mode's options, the others their defaults.
    pub options: Vec<OptionValue>,
    pub server: ServerSetup,
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
    /// How many seats the session plays.
    pub seats: usize,
}

impl AsRef<Offering> for Lobby {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

impl Lobby {
    /// The segments of a server's seed chain: the most checkpoints a session may take, less one.
    const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();

    /// A new session of a server, as `new` opens it, its files kept under `data`: every seat
    /// the mode has, its seed chain fresh from the server's entropy, at the mode's default rate,
    /// its inputs within `InputRules::LAN`. An error when it does not open or keeps no files.
    pub fn open(opening: LobbyOpening, data: &ServerDir) -> Result<Self, LobbyOpenError> {
        let root = opening.server.os.entropy.random();
        let manifest = opening.packages.manifest();
        let seats = usize::try_from(manifest.seats()).expect("seats fit usize");
        let tick_hz = manifest.tick_hz.default();
        let mut lobby = Self::new(LobbySetup {
            opening,
            seed_chain: SeedChain::new(root, Self::SEGMENTS),
            tick_hz,
            inputs: InputRules::LAN,
            seats,
        })?;
        lobby.keep_files(data)?;
        Ok(lobby)
    }

    /// A session of the mode `packages` holds, by the setup's rules; an error when the mode does
    /// not run at the setup's tick rate or has not as many seats, when a bot or an open seat
    /// names a seat the session does not have or one already named, when no seat is left to a
    /// player, when the host sets a visibility the mode does not leave it, and when it picks a
    /// value of no option of the mode's.
    pub fn new(setup: LobbySetup) -> Result<Self, LobbyError> {
        let LobbySetup {
            opening:
                LobbyOpening {
                    packages,
                    bots,
                    open,
                    host,
                    options,
                    server,
                },
            seed_chain,
            tick_hz,
            inputs,
            seats,
        } = setup;
        let mut plan = vec![SeatPlan::Player; seats];
        let named = (bots.seats.iter().map(|bot| (bot.seat, SeatPlan::Bot)))
            .chain(open.into_iter().map(|seat| (seat, SeatPlan::Open)));
        for (seat, kind) in named {
            let at = plan
                .get_mut(seat.index())
                .ok_or(LobbyError::NoSuchSeat(seat))?;
            if *at != SeatPlan::Player {
                return Err(LobbyError::SeatNamedTwice(seat));
            }
            *at = kind;
        }
        let players = plan
            .iter()
            .filter(|&&plan| plan == SeatPlan::Player)
            .count();
        if players == 0 {
            return Err(LobbyError::NoPlayer);
        }
        let session_rules = SessionRules::of(&packages);
        let visibility = session_rules
            .visibility()
            .session(&host)
            .map_err(LobbyError::Visibility)?;
        let picks =
            OptionPicks::named(session_rules.options(), &options).map_err(LobbyError::Options)?;
        let terms = session_rules
            .terms(HostChoices {
                server_key: server.signer.public_key(),
                seed_commitment: seed_chain.commitment(),
                tick_hz,
                inputs,
                seats: plan,
                visibility: &visibility,
                picks: &picks,
            })
            .map_err(LobbyError::Terms)?;
        Ok(Self {
            offering: Offering::new(terms, &server),
            seed_chain,
            packages,
            players,
            bots,
            server,
            joined: Vec::with_capacity(players),
            checked: CheckedTerms { visibility, picks },
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

    /// The setup of the server that opened it.
    pub const fn server(&self) -> &ServerSetup {
        &self.server
    }

    /// How many players joined so far.
    const fn joined(&self) -> usize {
        self.joined.len()
    }

    /// Takes each offered link's join: a player who answered their challenge joins in the next
    /// free seat, or the seat their main key held, whose older link ends; one who answered to
    /// watch takes a spectator's place while one is free; a refused link keeps its reason. The
    /// joins of one frame take their seats in `Offering::check_all`'s order.
    pub(crate) fn take_joins(
        mut lobby: ResMut<'_, Self>,
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
            match Spectators::place(&lobby.checked.visibility, watching) {
                Ok(viewer) => {
                    commands.entity(link).insert((Joined, viewer));
                    watching += 1;
                    info!(link = link.to_bits(), "a spectator joined");
                }
                Err(error) => Offering::refuse(&mut commands, link, error),
            }
        }
        for (link, delegation) in checked.answered.drain(..) {
            match lobby.seat(link, delegation) {
                Ok(older) => {
                    info!(
                        link = link.to_bits(),
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

    /// Starts the match once every seat is taken, and closes the lobby: the door takes the
    /// joins from then on.
    pub(crate) fn start_when_full(world: &mut World) {
        if world.resource::<Self>().joined.len() < world.resource::<Self>().players {
            return;
        }
        let lobby = world.remove_resource::<Self>().expect("the lobby is open");
        let terms = SessionTerms::clone(&lobby.offering.terms);
        let mut joined = lobby.joined.into_iter();
        let mut links = Vec::with_capacity(lobby.players);
        let mut seats = Vec::with_capacity(terms.seats.len());
        for (seat, plan) in (0..).map(Seat::new).zip(&terms.seats) {
            seats.push(match plan {
                SeatPlan::Player => {
                    let (link, delegation) = joined.next().expect("a player for each seat");
                    links.push((seat, link));
                    SeatStart::player(delegation)
                }
                SeatPlan::Bot => SeatStart::Bot,
                SeatPlan::Open => SeatStart::Open,
            });
        }
        let header = SessionHeader { terms, seats };
        let session = header.terms.session_id();
        let log = SessionLog::new(header)
            .expect("every delegation names this session, one main key each");
        info!(%session, players = links.len(), "every seat is taken; the match starts");
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
        ServerPhase::go(world, ServerPhase::Running);
        world.start_spectators();
    }

    /// Frees the seat of each joined link that is no longer connected, so a match starts only
    /// with live links, and the seat goes to the next player who joins.
    pub(crate) fn free_seats(
        mut lobby: ResMut<'_, Self>,
        live: Query<'_, '_, (), (With<Connected>, With<Joined>)>,
        mut commands: Commands<'_, '_>,
    ) {
        lobby.joined.retain(|&(link, _)| {
            let connected = live.contains(link);
            if !connected {
                info!(
                    link = link.to_bits(),
                    "a player left the lobby; their seat is free"
                );
                if let Ok(mut gone) = commands.get_entity(link) {
                    gone.remove::<Joined>();
                }
            }
            connected
        });
    }

    /// Seats the player of `link`, whose `delegation` the offering checked: in the seat their main
    /// key holds, whose older link it gives, or else in the next free seat.
    fn seat(&mut self, link: Entity, delegation: Delegation) -> Result<Option<Entity>, JoinError> {
        let main_key = delegation.main_key();
        if let Some(joined) = self
            .joined
            .iter_mut()
            .find(|(_, seated)| seated.main_key() == main_key)
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

#[cfg(test)]
mod tests;
