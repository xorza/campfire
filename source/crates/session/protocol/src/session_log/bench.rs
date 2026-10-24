use std::hint::black_box;
use std::num::NonZeroU32;

use campfire_common::{MapName, PackageFingerprint, PlayerSlot, Tick, Version};
use campfire_measure::{Kind, Scale};
use criterion::{BatchSize, Criterion, Throughput};

use crate::delegation::seed_contribution::SeedContribution;
use crate::delegation::{Delegation, DelegationTerms};
use crate::harness::test_key::TestKey;
use crate::input_chain::InputChain;
use crate::input_rules::InputRules;
use crate::player_slot_plan::PlayerSlotPlan;
use crate::player_slot_start::PlayerSlotStart;
use crate::seed_chain::SeedChain;
use crate::session_log::SessionLog;
use crate::session_log::player_packet::PlayerPacket;
use crate::session_log::session_header::SessionHeader;
use crate::session_terms::SessionTerms;
use crate::signature::Signature;

/// One tick's packets as the server records them, each player's one input of 8 bytes signed over
/// its chain head: the signature checks of a frame, one player slot after another, with 8 players
/// and with 32, its throughput the packets. Each run records into a log fresh from its header,
/// which the timing leaves out.
pub(crate) fn record_tick(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "session_log", Scale::Sample);
    for players in [8, 32] {
        let tick = TickPackets::new(players);
        group.throughput(Throughput::Elements(u64::from(players)));
        let count = usize::try_from(players).expect("a session's players fit usize");
        let case = campfire_measure::case("record_tick", count, "player");
        group.bench_function(case, |b| {
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
            player_slots: vec![
                PlayerSlotPlan::Player;
                usize::try_from(players).expect("player slots fit usize")
            ],
            visibility: Vec::new(),
            options: Vec::new(),
            player_slot_picks: Vec::new(),
        };
        let session_id = terms.session_id();
        // Each player's main key and session key take a byte of their own, past the server's.
        let key = |player_slot: u32, of: u32| {
            TestKey::of(u8::try_from(60 + 2 * player_slot + of).expect("a key"))
        };
        let delegations: Vec<Delegation> = (0..players)
            .map(|player_slot| {
                let granted = DelegationTerms {
                    session_key: key(player_slot, 1).x_only_public_key().0,
                    server_key: terms.server_key,
                    session_id,
                    seed_contribution: SeedContribution::new([4; 32]),
                    expiration: TestKey::NOW + 86_400,
                };
                Delegation::sign(
                    &TestKey::signer(key(player_slot, 0)),
                    &granted,
                    TestKey::NOW,
                )
            })
            .collect();
        let signatures = (0..players)
            .zip(&delegations)
            .map(|(player_slot, delegation)| {
                let mut chain =
                    InputChain::new(PlayerSlot::new(player_slot), delegation.chain_root());
                chain.extend(Tick::ZERO, PAYLOAD);
                let signer = TestKey::signer(key(player_slot, 1));
                signer.sign(&chain.signed_head(session_id))
            })
            .collect();
        let player_slots = delegations
            .into_iter()
            .map(PlayerSlotStart::player)
            .collect();
        Self {
            header: SessionHeader {
                terms,
                player_slots,
            },
            signatures,
        }
    }

    /// Records each player's packet into `log`, by player slot.
    fn record(&self, log: &mut SessionLog) {
        let mut applied = Vec::new();
        for (player_slot, signature) in (0..).zip(&self.signatures) {
            let packet = PlayerPacket {
                player_slot: PlayerSlot::new(player_slot),
                inputs: [(Tick::ZERO, PAYLOAD)],
                signature,
            };
            log.record(packet, &mut applied, None)
                .expect("the bench's packet holds");
        }
    }
}
