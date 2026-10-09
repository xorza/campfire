use std::cell::Cell;
use std::num::NonZeroU32;

use campfire_common::{Fingerprint, MapName};
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::internals::TestKey;
use campfire_protocol::secp256k1::XOnlyPublicKey;
use campfire_protocol::{
    CertificateHash, ConnectChallenge, InputHash, Receipt, SeedChain, SlotPlan,
};
use campfire_runner::{InputRules, TermsError};

use super::*;
use crate::match_start::ChainHead;

const NOW: u64 = 1_700_000_000;

thread_local! {
    /// The client's clock in this test, in Unix seconds.
    static CLOCK: Cell<u64> = const { Cell::new(NOW) };
}

fn clock() -> u64 {
    CLOCK.with(Cell::get)
}
const TICK_HZ: NonZeroU32 = NonZeroU32::new(30).unwrap();

type Change = fn(&mut SessionTerms);

/// The rules of the lane mode, which runs at 30 Hz only.
fn lane_rules() -> SessionRules {
    let dir = PackageDir::workspace("test/modes/lane");
    SessionRules::of(&ModePackages::from_dir(&dir, &MapName::new("lane").unwrap()).unwrap())
}

/// The x-only key of `TestKey::of(secret)`.
fn x_only(secret: u8) -> XOnlyPublicKey {
    TestKey::of(secret).x_only_public_key().0
}

/// A client of the lane mode, main key 1, that means to reach the remote server of key 8 at
/// `tick_hz`.
fn waiting(tick_hz: NonZeroU32) -> JoinState {
    let server = ServerPin {
        key: x_only(8),
        certificate: CertificateHash::new([3; 32]),
        tick_hz,
    };
    JoinState::new(TestKey::of(1), server, false, lane_rules(), clock)
}

/// Session key 2, whose randomness is all 6s.
fn signer() -> Signer {
    Signer::new(TestKey::of(2), |bytes| bytes.fill(6))
}

/// Terms the client can play, with `change` applied.
fn offer(change: impl FnOnce(&mut SessionTerms)) -> Offer {
    let mut terms = lane_rules()
        .terms(
            x_only(8),
            SeedChain::new([7; 32], NonZeroU32::MIN).commitment(),
            TICK_HZ,
            InputRules::LAN,
            vec![SlotPlan::Player; 2],
        )
        .unwrap();
    change(&mut terms);
    Offer {
        terms,
        times: SessionTimes::DEFAULT,
        challenge: ConnectChallenge::new([9; 32]),
    }
}

/// The start of the match in slot 1, of team 1, at sim tick 0, Lightyear tick 40, with a new
/// chain.
const START: MatchStart = MatchStart {
    start_tick: 40,
    first: Tick::new(0),
    slot: PlayerSlot::new(1),
    team: Team::new(1),
    chain: None,
    loaded: false,
};

#[test]
fn a_client_joins_only_the_session_its_server_offers_and_it_can_play() {
    let mut signer = signer();
    let other_rate = NonZeroU32::new(60).unwrap();
    let cases: [(NonZeroU32, Change, TermsMismatch); 6] = [
        (
            TICK_HZ,
            |terms| terms.server_key = x_only(9),
            TermsMismatch::OtherServer,
        ),
        (
            TICK_HZ,
            |terms| terms.release = "0.0.9".to_owned(),
            TermsMismatch::Terms(TermsError::OtherRelease("0.0.9".to_owned())),
        ),
        (
            TICK_HZ,
            |terms| terms.mode = Fingerprint::new([0; 32]),
            TermsMismatch::Terms(TermsError::OtherMode),
        ),
        (
            TICK_HZ,
            |terms| terms.dependencies.clear(),
            TermsMismatch::Terms(TermsError::OtherDependencies),
        ),
        // The listing names 60 Hz, and the lane mode does not run at it.
        (
            other_rate,
            |terms| terms.tick_hz = NonZeroU32::new(60).unwrap(),
            TermsMismatch::Terms(TermsError::TickRate(other_rate)),
        ),
        // The mode runs at 30 Hz, and the listing names 60.
        (other_rate, |_| {}, TermsMismatch::OtherTickRate),
    ];
    for (pinned, change, mismatch) in cases {
        let mut state = waiting(pinned);
        assert_eq!(
            state.answer(&offer(change), &mut signer),
            Some(Err(mismatch.clone()))
        );
        assert_eq!(state.refusal(), Some(&mismatch));
        // A refused client answers no later offer, and starts no match.
        assert_eq!(state.answer(&offer(|_| {}), &mut signer), None);
        assert_eq!(state.start(START), Started::No);
    }

    // A fitting offer: the delegation names the offered session, lets session key 2 sign until a
    // day after now, and carries the contribution the entropy gave; the answer passes the
    // server's check.
    let mut state = waiting(TICK_HZ);
    assert_eq!(
        state.start(START),
        Started::No,
        "no match starts before the answer"
    );
    let offer = offer(|_| {});
    let join = state.answer(&offer, &mut signer).unwrap().unwrap();
    assert_eq!(
        state.answer(&offer, &mut signer),
        None,
        "the client answers once"
    );
    let delegation = Delegation::parse(&join.delegation).unwrap();
    let granted = delegation.terms();
    assert_eq!(granted.session_id, offer.terms.session_id());
    assert_eq!(granted.session_key, TestKey::of(2).x_only_public_key().0);
    assert_eq!(
        delegation.main_key(),
        &TestKey::of(1).x_only_public_key().0.serialize()
    );
    assert_eq!(
        (granted.seed_contribution, granted.expiration),
        (SeedContribution::new([6; 32]), NOW + 86_400)
    );
    let check = |certificate| {
        offer.challenge.check(
            &Secp256k1::verification_only(),
            &offer.terms,
            &certificate,
            &delegation,
            &join.answer,
            NOW,
        )
    };
    assert_eq!(check(CertificateHash::new([3; 32])), Ok(()));
    assert!(check(CertificateHash::new([4; 32])).is_err());
    assert_eq!(state.clock(), None);
    assert_eq!((state.slot(), state.team()), (None, None));

    // The match starts once: sim tick 0 is Lightyear tick 40, and the player's chain in slot 1
    // starts from the delegation's id.
    assert_eq!(state.start(START), Started::Playing { discarded: 0 });
    assert_eq!(state.start(START), Started::No);
    assert_eq!(
        (state.slot(), state.team()),
        (Some(PlayerSlot::new(1)), Some(Team::new(1)))
    );
    let clock = state.clock().unwrap();
    assert_eq!(
        [39, 40, 42].map(|tick| clock.sim_tick(NetTick(tick))),
        [None, Some(Tick::new(0)), Some(Tick::new(2))]
    );
    let playing = state.playing_mut().unwrap();
    assert_eq!(playing.member.session.id, offer.terms.session_id());
    assert_eq!(
        playing.chain,
        InputChain::new(PlayerSlot::new(1), delegation.chain_root())
    );
}

/// A client that played: its chain in slot 1 holds the inputs of `payloads`, stamped 0; with the
/// heads after each and the delegation it answered with.
fn played(payloads: &[&[u8]]) -> (JoinState, Signer, Vec<InputHash>, Delegation) {
    let mut signer = signer();
    let mut state = waiting(TICK_HZ);
    let join = state.answer(&offer(|_| {}), &mut signer).unwrap().unwrap();
    state.start(START);
    let playing = state.playing_mut().unwrap();
    let mut heads = vec![playing.chain.head()];
    for &payload in payloads {
        playing.extend(Tick::new(0), payload);
        heads.push(playing.chain.head());
    }
    (
        state,
        signer,
        heads,
        Delegation::parse(&join.delegation).unwrap(),
    )
}

#[test]
fn a_client_whose_link_failed_takes_its_chain_up_again_from_the_servers_copy() {
    // Its link fails 10 s into its clock: it tries again after a wait drawn below 1 s, here all
    // of it but a nanosecond.
    let (mut state, mut signer, heads, delegation) = played(&[b"a", b"b", b"c"]);
    let lost = Duration::from_secs(10);
    assert_eq!(state.lose(lost, [0xFF; 32]), LinkLoss::Rejoining);
    assert!(state.rejoining() && state.clock().is_none());
    assert_eq!(state.lose(lost, [0xFF; 32]), LinkLoss::Nothing);
    let at = |nanos| lost + Duration::from_nanos(nanos);
    assert_eq!(state.retry(at(999_999_998), [0; 32]), Retry::Wait);
    assert_eq!(state.retry(at(999_999_999), [0; 32]), Retry::Connect);
    // The next wait, drawn as 0, is none.
    assert_eq!(state.retry(at(999_999_999), [0; 32]), Retry::Connect);

    // An offer of the same session: it answers with its own delegation, which has not expired.
    let join = state.answer(&offer(|_| {}), &mut signer).unwrap().unwrap();
    assert_eq!(join.delegation, delegation.json());
    // The server's copy holds 2 of its 3 inputs: the third never applies, and the chain goes on
    // from the second, at sim tick 500, Lightyear tick 7.
    let head = ChainHead {
        next_seq: 2,
        head: heads[2],
    };
    let resumed = MatchStart {
        start_tick: 7,
        first: Tick::new(500),
        chain: Some(head),
        ..START
    };
    assert_eq!(state.start(resumed), Started::Playing { discarded: 1 });
    let playing = state.playing().unwrap();
    assert_eq!(
        playing.chain,
        InputChain::resume(PlayerSlot::new(1), heads[2], 2)
    );
    assert_eq!(playing.clock.sim_tick(NetTick(8)), Some(Tick::new(501)));

    // Again: a server whose copy is not the client's own ends it.
    assert_eq!(state.lose(lost, [0; 32]), LinkLoss::Rejoining);
    state.answer(&offer(|_| {}), &mut signer).unwrap().unwrap();
    let rewritten = ChainHead {
        next_seq: 1,
        head: heads[2],
    };
    let start = MatchStart {
        chain: Some(rewritten),
        ..resumed
    };
    assert_eq!(state.start(start), Started::Rewritten);
    assert_eq!(state.loss(), Some(Loss::Rewritten));
}

#[test]
fn a_client_renews_an_expiring_delegation_and_gives_up_past_the_servers_times() {
    // A day after the join, its delegation expires within the margin: the client answers with a
    // new delegation, by its main key, of a new session key.
    let (mut state, mut signer, _, delegation) = played(&[b"a"]);
    CLOCK.with(|clock| clock.set(NOW + 86_400 - 60));
    assert_eq!(state.lose(Duration::ZERO, [0; 32]), LinkLoss::Rejoining);
    let join = state.answer(&offer(|_| {}), &mut signer).unwrap().unwrap();
    let renewed = Delegation::parse(&join.delegation).unwrap();
    assert_eq!(renewed.main_key(), delegation.main_key());
    assert_ne!(renewed.terms().session_key, delegation.terms().session_key);
    assert_eq!(renewed.terms().session_key, signer.public_key());
    assert_eq!(renewed.terms().expiration, NOW + 86_400 - 60 + 86_400);
    CLOCK.with(|clock| clock.set(NOW));

    // An offer of another session ends it.
    let (mut state, mut signer, _, _) = played(&[]);
    state.lose(Duration::ZERO, [0; 32]);
    let other = offer(|terms| terms.max_payload_len += 1);
    assert_eq!(
        state.answer(&other, &mut signer),
        Some(Err(TermsMismatch::OtherSession))
    );

    // Past the grace period and the restore window, 3 minutes, since its link failed, it stops.
    let (mut state, _, _, _) = played(&[]);
    state.lose(Duration::ZERO, [0; 32]);
    assert_eq!(
        state.retry(Duration::from_secs(180), [0; 32]),
        Retry::Connect
    );
    let past = Duration::from_secs(180) + Duration::from_nanos(1);
    assert_eq!(state.retry(past, [0; 32]), Retry::GiveUp);
    assert_eq!(state.loss(), Some(Loss::GaveUp));

    // A link that fails before any offer stops the client at once.
    let mut state = waiting(TICK_HZ);
    assert_eq!(
        state.lose(Duration::ZERO, [0; 32]),
        LinkLoss::Stopped(Loss::LinkFailed)
    );
}

#[test]
fn each_wait_is_drawn_below_a_bound_that_doubles_to_8_s() {
    let bounds = [0, 1, 2, 3, 4, 9].map(|tries| JoinState::wait(tries, [0xFF; 32]));
    let less_a_nanosecond = |seconds| {
        Duration::from_secs(seconds)
            .checked_sub(Duration::from_nanos(1))
            .unwrap()
    };
    assert_eq!(bounds, [1, 2, 4, 8, 8, 8].map(less_a_nanosecond));
    assert_eq!(JoinState::wait(2, [0; 32]), Duration::ZERO);
    // Half the draws' range: half the bound.
    let mut half = [0; 32];
    half[7] = 0x80;
    assert_eq!(JoinState::wait(1, half), Duration::from_secs(1));
}

/// `receipt` with the server key's signature over it.
fn signed(receipt: Receipt) -> SignedReceipt {
    let server = TestKey::of(8);
    SignedReceipt {
        receipt,
        signature: receipt.sign(&Secp256k1::new(), &server, &[0; 32]),
    }
}

/// A client that played inputs a, b and c, seqs 0, 1 and 2, `heads[n]` the head after the first
/// `n` of them; with a receipt of b, seq 1, at tick 5.
fn receipted() -> (JoinState, Signer, Vec<InputHash>, Delegation, Receipt) {
    let (state, signer, heads, delegation) = played(&[b"a", b"b", b"c"]);
    let receipt = Receipt {
        session_id: offer(|_| {}).terms.session_id(),
        slot: PlayerSlot::new(1),
        delegation: *delegation.id(),
        tick: Tick::new(5),
        seq: 1,
        head: heads[2],
    };
    (state, signer, heads, delegation, receipt)
}

#[test]
fn a_client_refuses_a_receipt_not_signed_over_its_own_chain() {
    let verifier = Secp256k1::verification_only();
    let (mut state, _, heads, _, receipt) = receipted();
    assert_eq!(
        waiting(TICK_HZ).take_receipt(&signed(receipt), &verifier),
        Err(ReceiptRefusal::NotPlaying)
    );
    let forged = SignedReceipt {
        signature: signed(Receipt { seq: 0, ..receipt }).signature,
        ..signed(receipt)
    };
    let refused = [
        (forged, ReceiptRefusal::BadSignature),
        (
            signed(Receipt {
                session_id: SessionId::new([1; 32]),
                ..receipt
            }),
            ReceiptRefusal::Other,
        ),
        (
            signed(Receipt {
                slot: PlayerSlot::new(0),
                ..receipt
            }),
            ReceiptRefusal::Other,
        ),
        (
            signed(Receipt {
                delegation: DelegationId::new([0; 32]),
                ..receipt
            }),
            ReceiptRefusal::Other,
        ),
        (
            signed(Receipt {
                head: heads[1],
                ..receipt
            }),
            ReceiptRefusal::OtherHead,
        ),
        // Seq 3 is past the chain's last input.
        (
            signed(Receipt {
                seq: 3,
                head: heads[3],
                ..receipt
            }),
            ReceiptRefusal::OtherHead,
        ),
        (
            signed(Receipt {
                seq: u64::MAX,
                ..receipt
            }),
            ReceiptRefusal::OtherHead,
        ),
    ];
    for (receipt, refusal) in refused {
        assert_eq!(state.take_receipt(&receipt, &verifier), Err(refusal));
    }
    assert_eq!(state.receipt(), None);
}

#[test]
fn only_a_client_of_a_local_server_takes_a_loaded_chain_as_the_server_holds_it() {
    // After a load, the server's chain stands at seq 1 with a head that is not the client's own
    // there, heads[1]: a local server's chain is taken, its 2 inputs after seq 1 never apply and
    // its receipt is dropped; any other server rewrote the chain.
    let verifier = Secp256k1::verification_only();
    for local in [true, false] {
        let (mut state, mut signer, heads, _, receipt) = receipted();
        state.player.local = local;
        state.take_receipt(&signed(receipt), &verifier).unwrap();
        state.lose(Duration::ZERO, [0; 32]);
        state.answer(&offer(|_| {}), &mut signer).unwrap().unwrap();
        let loaded = MatchStart {
            chain: Some(ChainHead {
                next_seq: 1,
                head: heads[3],
            }),
            loaded: true,
            ..START
        };
        if local {
            assert_eq!(state.start(loaded), Started::Playing { discarded: 2 });
            let playing = state.playing().unwrap();
            assert_eq!(
                playing.chain,
                InputChain::resume(PlayerSlot::new(1), heads[3], 1)
            );
            assert_eq!(state.receipt(), None);
        } else {
            assert_eq!(state.start(loaded), Started::Rewritten);
        }
    }
}

#[test]
fn a_client_keeps_its_newest_receipt_through_a_rejoin_and_a_renewal() {
    let verifier = Secp256k1::verification_only();
    let (mut state, mut signer, heads, delegation, receipt) = receipted();
    // Seq 1 is kept; seq 1 again, as a restored server gives it at a later tick, is kept in its
    // place, and with another head refused; seq 0 is older; seq 2 replaces it.
    assert_eq!(state.take_receipt(&signed(receipt), &verifier), Ok(()));
    assert_eq!(state.receipt(), Some(&signed(receipt)));
    let again = Receipt {
        tick: Tick::new(9),
        ..receipt
    };
    assert_eq!(state.take_receipt(&signed(again), &verifier), Ok(()));
    assert_eq!(state.receipt(), Some(&signed(again)));
    let other = Receipt {
        head: heads[1],
        ..again
    };
    assert_eq!(
        state.take_receipt(&signed(other), &verifier),
        Err(ReceiptRefusal::OtherHead)
    );
    let first = Receipt {
        seq: 0,
        head: heads[1],
        ..receipt
    };
    assert_eq!(
        state.take_receipt(&signed(first), &verifier),
        Err(ReceiptRefusal::Older)
    );
    let third = Receipt {
        tick: Tick::new(6),
        seq: 2,
        head: heads[3],
        ..receipt
    };
    assert_eq!(state.take_receipt(&signed(third), &verifier), Ok(()));
    assert_eq!(state.receipt(), Some(&signed(third)));

    // A day on, the link fails: the client keeps the receipt while it tries again, renews its
    // delegation, and resumes from the server's copy of all three inputs.
    let failed = Duration::from_secs(10);
    assert_eq!(state.lose(failed, [0; 32]), LinkLoss::Rejoining);
    assert_eq!(state.receipt(), Some(&signed(third)));
    CLOCK.with(|clock| clock.set(NOW + 86_400 - 60));
    let join = state.answer(&offer(|_| {}), &mut signer).unwrap().unwrap();
    CLOCK.with(|clock| clock.set(NOW));
    let renewed = Delegation::parse(&join.delegation).unwrap();
    assert_ne!(delegation.id(), renewed.id());
    let resumed = MatchStart {
        chain: Some(ChainHead {
            next_seq: 3,
            head: heads[3],
        }),
        ..START
    };
    assert_eq!(state.start(resumed), Started::Playing { discarded: 0 });
    assert_eq!(state.receipt(), Some(&signed(third)));

    // Inputs d and e, seqs 3 and 4: a receipt names the renewed delegation, or the one it renewed,
    // which signed the inputs before it; no other.
    let playing = state.playing_mut().unwrap();
    playing.extend(Tick::new(0), b"d");
    let fourth = Receipt {
        seq: 3,
        head: playing.chain.head(),
        ..third
    };
    let stranger = Receipt {
        delegation: DelegationId::new([0; 32]),
        ..fourth
    };
    assert_eq!(
        state.take_receipt(&signed(stranger), &verifier),
        Err(ReceiptRefusal::Other)
    );
    assert_eq!(state.take_receipt(&signed(fourth), &verifier), Ok(()));
    let playing = state.playing_mut().unwrap();
    playing.extend(Tick::new(0), b"e");
    let fifth = Receipt {
        delegation: *renewed.id(),
        seq: 4,
        head: playing.chain.head(),
        ..fourth
    };
    assert_eq!(state.take_receipt(&signed(fifth), &verifier), Ok(()));

    // The history forgot every head before the receipt's: a server whose copy holds fewer
    // inputs than it named rewrote the chain.
    assert_eq!(state.lose(failed, [0; 32]), LinkLoss::Rejoining);
    state.answer(&offer(|_| {}), &mut signer).unwrap().unwrap();
    let start = MatchStart {
        chain: Some(ChainHead {
            next_seq: 4,
            head: fourth.head,
        }),
        ..START
    };
    assert_eq!(state.start(start), Started::Rewritten);
}

#[test]
fn a_client_that_left_takes_a_receipt_until_its_link_closes() {
    // The player leaves after inputs a, b and c; the receipt of c, seq 2, which the server sent
    // before it took the leave, comes after it: the client keeps it, as the newest. Once the
    // link closes, it takes none.
    let verifier = Secp256k1::verification_only();
    let (mut state, _, heads, _, receipt) = receipted();
    state.take_receipt(&signed(receipt), &verifier).unwrap();
    state.leave();
    assert!(state.left());
    let last = Receipt {
        seq: 2,
        head: heads[3],
        ..receipt
    };
    assert_eq!(state.take_receipt(&signed(last), &verifier), Ok(()));
    assert_eq!(state.receipt(), Some(&signed(last)));
    assert_eq!(state.lose(Duration::ZERO, [0; 32]), LinkLoss::Nothing);
    assert!(state.left());
    assert_eq!(
        state.take_receipt(&signed(last), &verifier),
        Err(ReceiptRefusal::NotPlaying)
    );
    // A client that leaves before it plays holds no chain to take one of.
    let mut early = waiting(TICK_HZ);
    early.leave();
    assert_eq!(
        early.take_receipt(&signed(last), &verifier),
        Err(ReceiptRefusal::NotPlaying)
    );
}
