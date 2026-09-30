use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_package::{ModePackages, RELEASE};
use campfire_protocol::secp256k1::{Secp256k1, VerifyOnly};
use campfire_protocol::{
    CertificateHash, ConnectChallenge, Delegation, SeedChain, SessionHeader, SessionLog,
    SessionTerms,
};
use campfire_runner::Session;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    Connected, LocalTimeline, MessageReceiver, MessageSender, Tick as NetTick,
};
use tracing::{debug, info, warn};

use crate::error::JoinError;
use crate::join::Join;
use crate::net_protocol::MatchChannel;
use crate::offer::Offer;
use crate::sim_server::SimServer;

/// The most ticks an input may land after its stamp.
const MAX_INPUT_DELAY: u64 = 10;
/// The most ticks an input's stamp may be ahead of the next tick.
const MAX_INPUT_LEAD: u64 = 30;
const MAX_PAYLOAD_LEN: u32 = 64;
const MAX_INPUTS_PER_TICK: u32 = 4;
/// Ticks between two sends of an offer that got no answer.
const RESEND_TICKS: u32 = 30;

/// A session open for players to join: the server offers each connected client the terms and a
/// challenge, checks each answer, gives the players slots in the order they joined, and starts
/// the match when every slot is taken.
#[derive(Resource, Debug)]
pub struct Lobby {
    terms: SessionTerms,
    seed_chain: SeedChain,
    certificate: CertificateHash,
    packages: ModePackages,
    players: usize,
    clock: fn() -> u64,
    entropy: fn(&mut [u8; 32]),
    secp: Secp256k1<VerifyOnly>,
    /// The links that joined, by slot, with their delegations.
    joined: Vec<(Entity, Delegation)>,
}

/// What a server opens a session with.
#[derive(Debug)]
pub struct LobbySetup {
    pub packages: ModePackages,
    /// The server's x-only public key.
    pub server_key: [u8; 32],
    pub seed_chain: SeedChain,
    /// The hash of the TLS certificate the server's transport presents.
    pub certificate: CertificateHash,
    pub players: usize,
    /// Unix seconds, against which a delegation's expiry is checked.
    pub clock: fn() -> u64,
    /// Fills a challenge with random bytes.
    pub entropy: fn(&mut [u8; 32]),
}

/// The challenge a link was offered, and the tick the offer last went out in.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct Offered {
    challenge: ConnectChallenge,
    sent: NetTick,
}

/// A link whose player joined the session.
#[derive(Component, Debug, Clone, Copy)]
pub struct Joined;

/// The connected links still to answer, with their offer if one went out.
type OfferLinks<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static mut Offered>,
        &'static mut MessageSender<Offer>,
    ),
    (
        With<Connected>,
        With<ClientOf>,
        Without<Joined>,
        Without<JoinRefused>,
    ),
>;

/// The offered links still to answer.
type JoinLinks<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Offered, &'static mut MessageReceiver<Join>),
    (Without<Joined>, Without<JoinRefused>),
>;

/// Why the server refused a link's join. The link stays connected and receives nothing more.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinRefused(pub JoinError);

impl Lobby {
    /// A session of the mode `packages` holds at its default tick rate, with the LAN's input
    /// limits.
    pub fn new(setup: LobbySetup) -> Lobby {
        let LobbySetup {
            packages,
            server_key,
            seed_chain,
            certificate,
            players,
            clock,
            entropy,
        } = setup;
        assert!(players > 0, "a session has a player");
        let terms = SessionTerms {
            server_key,
            tick_hz: packages.manifest().tick_hz.default(),
            max_input_delay: MAX_INPUT_DELAY,
            max_input_lead: MAX_INPUT_LEAD,
            max_payload_len: MAX_PAYLOAD_LEN,
            max_inputs_per_tick: MAX_INPUTS_PER_TICK,
            seed_commitment: seed_chain.commitment(),
            release: RELEASE.to_owned(),
            mode: Session::mode_in_terms(&packages),
            dependencies: Session::dependencies_in_terms(&packages),
        };
        Lobby {
            terms,
            seed_chain,
            certificate,
            packages,
            players,
            clock,
            entropy,
            secp: Secp256k1::verification_only(),
            joined: Vec::with_capacity(players),
        }
    }

    pub const fn terms(&self) -> &SessionTerms {
        &self.terms
    }

    /// How many players joined so far.
    fn joined(&self) -> usize {
        self.joined.len()
    }

    /// Offers the terms and a fresh challenge to each connected client that has none, and the
    /// same offer again every `RESEND_TICKS` until it answers: a client may not yet receive
    /// messages when the link connects, and then the first offer is lost.
    pub(crate) fn offer(
        lobby: ResMut<'_, Lobby>,
        timeline: Res<'_, LocalTimeline>,
        mut links: OfferLinks<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = timeline.tick();
        for (link, offered, mut sender) in &mut links {
            let challenge = match offered {
                Some(offered) if now.0.wrapping_sub(offered.sent.0) < RESEND_TICKS => continue,
                Some(mut offered) => {
                    debug!(?link, "sent the offer again");
                    offered.sent = now;
                    offered.challenge
                }
                None => {
                    let mut challenge = [0; 32];
                    (lobby.entropy)(&mut challenge);
                    let challenge = ConnectChallenge::new(challenge);
                    commands.entity(link).insert(Offered {
                        challenge,
                        sent: now,
                    });
                    debug!(?link, "offered the terms");
                    challenge
                }
            };
            sender.send::<MatchChannel>(Offer {
                terms: lobby.terms.clone(),
                challenge,
            });
        }
    }

    /// Takes each offered link's join: a player who answered their challenge joins in the next
    /// free slot, and a refused link keeps its reason.
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
                Ok(()) => {
                    info!(
                        ?link,
                        joined = lobby.joined(),
                        of = lobby.players,
                        "a player joined"
                    );
                    commands.entity(link).insert(Joined);
                }
                Err(error) => {
                    warn!(?link, %error, "refused a join");
                    commands.entity(link).insert(JoinRefused(error));
                }
            }
        }
    }

    /// Starts the match once every slot is taken, and closes the lobby.
    pub(crate) fn start_when_full(world: &mut World) {
        if world.resource::<Lobby>().joined.len() < world.resource::<Lobby>().players {
            return;
        }
        let lobby = world.remove_resource::<Lobby>().expect("the lobby is open");
        let (links, players): (Vec<Entity>, Vec<Delegation>) = lobby.joined.into_iter().unzip();
        let header = SessionHeader {
            terms: lobby.terms,
            players,
        };
        let session = header.terms.session_id();
        let log = SessionLog::new(header).expect("every delegation names this session");
        info!(%session, players = links.len(), "every slot is taken; the match starts");
        let server_seed = lobby.seed_chain.seed(0);
        SimServer::start_match(world, log, server_seed, &lobby.packages, &links)
            .expect("the lobby's terms come from its own packages");
    }

    /// Seats the player of `link` in the next free slot if their `join` answers `challenge`.
    fn take(
        &mut self,
        link: Entity,
        challenge: ConnectChallenge,
        join: &Join,
    ) -> Result<(), JoinError> {
        let delegation = self.check(challenge, join)?;
        if self.joined.len() == self.players {
            return Err(JoinError::Full);
        }
        self.joined.push((link, delegation));
        Ok(())
    }

    fn check(&self, challenge: ConnectChallenge, join: &Join) -> Result<Delegation, JoinError> {
        let delegation = Delegation::parse(&join.delegation).map_err(JoinError::Delegation)?;
        challenge
            .check(
                &self.secp,
                &self.terms,
                &self.certificate,
                &delegation,
                &join.answer,
                (self.clock)(),
            )
            .map_err(JoinError::Connect)?;
        Ok(delegation)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::path::Path;

    use campfire_protocol::secp256k1::{Keypair, SecretKey};
    use campfire_protocol::{ConnectError, DelegationError, DelegationTerms};

    use super::*;

    const NOW: u64 = 1_700_000_000;

    fn keypair(byte: u8) -> Keypair {
        let secret = SecretKey::from_byte_array(&[byte; 32]).unwrap();
        Keypair::from_secret_key(&Secp256k1::new(), &secret)
    }

    #[test]
    fn the_lobby_takes_a_join_only_with_a_delegation_and_an_answer_for_it() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/test/modes/lane"
        );
        let mut lobby = Lobby::new(LobbySetup {
            packages: ModePackages::from_dir(Path::new(dir)).unwrap(),
            server_key: [8; 32],
            seed_chain: SeedChain::new([7; 32], NonZeroU32::MIN),
            certificate: CertificateHash::new([3; 32]),
            players: 2,
            clock: || NOW,
            entropy: |bytes| bytes.fill(5),
        });
        assert_eq!((lobby.players, lobby.joined()), (2, 0));
        let secp = Secp256k1::new();
        let granted = DelegationTerms {
            session_key: keypair(2).x_only_public_key().0,
            server_key: [8; 32],
            session_id: lobby.terms().session_id(),
            seed_contribution: [6; 32],
            expiration: NOW + 60,
        };
        let delegation = Delegation::sign(&secp, &keypair(1), &granted, NOW, &[0; 32]);
        let challenge = ConnectChallenge::new([9; 32]);
        let join = |certificate: [u8; 32], json: &str| Join {
            delegation: json.to_owned(),
            answer: challenge.answer(
                &secp,
                &keypair(2),
                &CertificateHash::new(certificate),
                &[0; 32],
            ),
        };
        assert_eq!(
            lobby.check(challenge, &join([3; 32], delegation.json())),
            Ok(delegation.clone())
        );
        // An answer over another certificate, as a relay to this server gives, and a delegation
        // that is not an event.
        assert_eq!(
            lobby.check(challenge, &join([4; 32], delegation.json())),
            Err(JoinError::Connect(ConnectError::BadAnswer))
        );
        assert_eq!(
            lobby.check(challenge, &join([3; 32], "{}")),
            Err(JoinError::Delegation(DelegationError::NotEvent))
        );

        // Two seats: a refused join takes none, and a third good join finds both taken.
        let good = join([3; 32], delegation.json());
        let links = [
            Entity::from_raw_u32(1),
            Entity::from_raw_u32(2),
            Entity::from_raw_u32(3),
        ]
        .map(Option::unwrap);
        assert!(
            lobby
                .take(links[0], challenge, &join([4; 32], delegation.json()))
                .is_err()
        );
        assert_eq!(lobby.joined(), 0);
        assert_eq!(lobby.take(links[0], challenge, &good), Ok(()));
        assert_eq!(lobby.take(links[1], challenge, &good), Ok(()));
        assert_eq!(lobby.take(links[2], challenge, &good), Err(JoinError::Full));
        let seated: Vec<Entity> = lobby.joined.iter().map(|&(link, _)| link).collect();
        assert_eq!(seated, links[..2]);
    }
}
