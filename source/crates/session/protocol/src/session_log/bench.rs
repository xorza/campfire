use std::hint::black_box;
use std::num::NonZeroU32;

use campfire_common::{MapName, PackageFingerprint, Seat, Tick, Version};
use criterion::{BatchSize, Criterion};

use crate::delegation::seed_contribution::SeedContribution;
use crate::delegation::{Delegation, DelegationTerms};
use crate::harness::test_key::TestKey;
use crate::input_chain::InputChain;
use crate::input_rules::InputRules;
use crate::seat_plan::SeatPlan;
use crate::seat_start::SeatStart;
use crate::seed_chain::SeedChain;
use crate::session_log::SessionLog;
use crate::session_log::player_packet::PlayerPacket;
use crate::session_log::session_header::SessionHeader;
use crate::session_terms::SessionTerms;
use crate::signature::Signature;

/// One tick's packets as the server records them, each player's one input of 8 bytes signed
/// over its chain head: the signature checks of a frame, one seat after another, with 8 players
/// and with 32. Each run records into a log fresh from its header, which the timing leaves
/// out.
pub(crate) fn record_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("integration/session_log");
    for players in [8, 32] {
        let tick = TickPackets::new(players);
        group.bench_function(format!("record_tick_{players}"), |b| {
            b.iter_batched(
                || SessionLog::new(tick.header.clone()).expect("the bench's header holds"),
                |mut log| {
                    tick.record(&mut log);
                    black_box(log)
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// A session of `players` players and the packet each sends before tick 0.
struct TickPackets {
    header: SessionHeader,
    signatures: Vec<Signature>,
}

/// What each player sends: one input, stamped tick 0.
const PAYLOAD: &[u8] = b"an order";

impl TickPackets {
    fn new(players: u32) -> Self {
        let server = TestKey::server();
        let terms = SessionTerms {
            server_key: server.x_only_public_key().0,
            tick_hz: NonZeroU32::new(30).expect("a rate"),
            inputs: InputRules::LAN,
            seed_commitment: SeedChain::new([9; 32], NonZeroU32::MIN).commitment(),
            release: Version::ENGINE,
            mode: PackageFingerprint::new([3; 32]),
            map: MapName::parse("lane").expect("a map's name"),
            dependencies: Vec::new(),
            seats: vec![SeatPlan::Player; usize::try_from(players).expect("seats fit usize")],
            visibility: Vec::new(),
            options: Vec::new(),
        };
        let session_id = terms.session_id();
        // Each player's main key and session key take a byte of their own, past the server's.
        let key =
            |seat: u32, of: u32| TestKey::of(u8::try_from(60 + 2 * seat + of).expect("a key"));
        let delegations: Vec<Delegation> = (0..players)
            .map(|seat| {
                let granted = DelegationTerms {
                    session_key: key(seat, 1).x_only_public_key().0,
                    server_key: terms.server_key,
                    session_id,
                    seed_contribution: SeedContribution::new([4; 32]),
                    expiration: TestKey::NOW + 86_400,
                };
                Delegation::sign(&TestKey::signer(key(seat, 0)), &granted, TestKey::NOW)
            })
            .collect();
        let signatures = (0..players)
            .zip(&delegations)
            .map(|(seat, delegation)| {
                let mut chain = InputChain::new(Seat::new(seat), delegation.chain_root());
                chain.extend(Tick::ZERO, PAYLOAD);
                let signer = TestKey::signer(key(seat, 1));
                signer.sign(&chain.signed_head(session_id))
            })
            .collect();
        let seats = delegations.into_iter().map(SeatStart::player).collect();
        Self {
            header: SessionHeader { terms, seats },
            signatures,
        }
    }

    /// Records each player's packet into `log`, by seat.
    fn record(&self, log: &mut SessionLog) {
        let mut applied = Vec::new();
        for (seat, signature) in (0..).zip(&self.signatures) {
            let packet = PlayerPacket {
                seat: Seat::new(seat),
                inputs: [(Tick::ZERO, PAYLOAD)],
                signature,
            };
            log.record(packet, &mut applied, None)
                .expect("the bench's packet holds");
        }
    }
}
