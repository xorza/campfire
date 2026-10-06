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
mod tests {
    use campfire_package::PackageDir;

    use crate::harness::in_process_match;
    use crate::order_script::OrderScript;
    use crate::session_times::SessionTimes;
    use crate::sim_server::server_bots::SlotBot;

    use bevy_ecs::system::RunSystemOnce;
    use campfire_protocol::secp256k1::Secp256k1;
    use campfire_protocol::{CertificateHash, ConnectError, DelegationError, DelegationTerms};
    use lightyear::prelude::{PeerId, RemoteId};

    use super::*;

    const NOW: u64 = 1_700_000_000;

    /// A session of the lane mode's 2 slots, with bots in slots `bots` and slots `open` open.
    fn setup(bots: &[u32], open: &[u32]) -> LobbySetup {
        let script = OrderScript::parse("end = 1").unwrap();
        LobbySetup {
            packages: Arc::new(
                ModePackages::from_dir(&PackageDir::workspace("test/modes/lane")).unwrap(),
            ),
            seed_chain: SeedChain::new([7; 32], NonZeroU32::MIN),
            tick_hz: NonZeroU32::new(30).unwrap(),
            inputs: InputRules::LAN,
            slots: 2,
            bots: ServerBots {
                slots: bots
                    .iter()
                    .map(|&slot| SlotBot::new(slot, script.clone()))
                    .collect(),
                takeover: None,
            },
            open: open.iter().copied().map(PlayerSlot::new).collect(),
            server: ServerSetup {
                key: in_process_match::server_keypair(),
                certificate: CertificateHash::new([3; 32]),
                times: SessionTimes::DEFAULT,
                clock: || NOW,
                entropy: |bytes| bytes.fill(5),
            },
        }
    }

    #[test]
    fn the_lobby_plans_each_slot_once_and_leaves_one_to_a_player() {
        let plan = |bots: &[u32], open: &[u32]| {
            Lobby::new(setup(bots, open)).map(|lobby| lobby.terms().slots.clone())
        };
        assert_eq!(plan(&[1], &[]), Ok(vec![SlotPlan::Player, SlotPlan::Bot]));
        assert_eq!(plan(&[], &[0]), Ok(vec![SlotPlan::Open, SlotPlan::Player]));
        let slot = PlayerSlot::new;
        assert_eq!(plan(&[2], &[]), Err(LobbyError::NoSuchSlot(slot(2))));
        assert_eq!(plan(&[1, 1], &[]), Err(LobbyError::SlotNamedTwice(slot(1))));
        assert_eq!(plan(&[1], &[1]), Err(LobbyError::SlotNamedTwice(slot(1))));
        assert_eq!(plan(&[0], &[1]), Err(LobbyError::NoPlayer));
    }

    #[test]
    fn the_lobby_takes_a_join_only_with_a_delegation_and_an_answer_for_it() {
        let server_key = in_process_match::server_key();
        let mut lobby = Lobby::new(setup(&[], &[])).unwrap();
        assert_eq!((lobby.players, lobby.joined()), (2, 0));
        let secp = Secp256k1::new();
        let granted = DelegationTerms {
            session_key: in_process_match::keypair(2).x_only_public_key().0,
            server_key,
            session_id: lobby.terms().session_id(),
            seed_contribution: [6; 32],
            expiration: NOW + 60,
        };
        let delegation = Delegation::sign(
            &secp,
            &in_process_match::keypair(1),
            &granted,
            NOW,
            &[0; 32],
        );
        let challenge = ConnectChallenge::new([9; 32]);
        let join = |certificate: [u8; 32], json: &str| Join {
            delegation: json.to_owned(),
            answer: challenge.answer(
                &secp,
                &in_process_match::keypair(2),
                &CertificateHash::new(certificate),
                &[0; 32],
            ),
        };
        assert_eq!(
            lobby
                .offering
                .check(challenge, &join([3; 32], delegation.json())),
            Ok(delegation.clone())
        );
        // An answer over another certificate, as a relay to this server gives, and a delegation
        // that is not an event.
        assert_eq!(
            lobby
                .offering
                .check(challenge, &join([4; 32], delegation.json())),
            Err(JoinError::Connect(ConnectError::BadAnswer))
        );
        assert_eq!(
            lobby.offering.check(challenge, &join([3; 32], "{}")),
            Err(JoinError::Delegation(DelegationError::NotEvent))
        );

        // Two seats: a refused join takes none; a second join of one main key, from another link,
        // takes its seat back, and gives its older link; and a third player finds both taken.
        let good = join([3; 32], delegation.json());
        let player = |main: u8| {
            let delegation = Delegation::sign(
                &secp,
                &in_process_match::keypair(main),
                &granted,
                NOW,
                &[0; 32],
            );
            join([3; 32], delegation.json())
        };
        let mut world = World::new();
        let links = [1, 2, 3, 4].map(|peer| {
            let remote = RemoteId(PeerId::Local(peer));
            world.spawn((remote, Connected, Joined)).id()
        });
        assert!(
            lobby
                .take(links[0], challenge, &join([4; 32], delegation.json()))
                .is_err()
        );
        assert_eq!(lobby.joined(), 0);
        assert_eq!(lobby.take(links[2], challenge, &good), Ok(None));
        assert_eq!(lobby.take(links[0], challenge, &good), Ok(Some(links[2])));
        assert_eq!(lobby.take(links[1], challenge, &player(3)), Ok(None));
        assert_eq!(
            lobby.take(links[2], challenge, &player(4)),
            Err(JoinError::Full)
        );
        let seated =
            |lobby: &Lobby| -> Vec<Entity> { lobby.joined.iter().map(|&(link, _)| link).collect() };
        assert_eq!(seated(&lobby), links[..2]);

        // The first player's link disconnects: their seat is free, their link no longer joined,
        // and a fourth player takes the seat.
        world.entity_mut(links[0]).remove::<Connected>();
        world.insert_resource(lobby);
        world.run_system_once(Lobby::free_seats).unwrap();
        assert!(!world.entity(links[0]).contains::<Joined>());
        let mut lobby = world.remove_resource::<Lobby>().unwrap();
        assert_eq!(seated(&lobby), [links[1]]);
        assert_eq!(lobby.take(links[3], challenge, &player(5)), Ok(None));
        assert_eq!(seated(&lobby), [links[1], links[3]]);
    }
}
