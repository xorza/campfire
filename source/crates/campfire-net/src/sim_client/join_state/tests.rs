use std::num::NonZeroU32;

use std::cell::Cell;

use campfire_common::{Fingerprint, PlayerSlot};
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::secp256k1::{Secp256k1, XOnlyPublicKey};
use campfire_protocol::{CertificateHash, ConnectChallenge, InputHash, SeedChain, SlotPlan};
use campfire_runner::{InputRules, TermsError};

use super::*;
use crate::local_match;
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
    SessionRules::of(&ModePackages::from_dir(&dir).unwrap())
}

/// An x-only key of the bytes `[byte; 32]`, which must be the x of a point on the curve.
fn x_only(byte: u8) -> XOnlyPublicKey {
    XOnlyPublicKey::from_byte_array(&[byte; 32]).unwrap()
}

/// A client of the lane mode, main key 1, that means to reach the server of key 8 at `tick_hz`.
fn waiting(tick_hz: NonZeroU32) -> JoinState {
    let server = ServerPin {
        key: x_only(8),
        certificate: CertificateHash::new([3; 32]),
        tick_hz,
    };
    JoinState::new(local_match::keypair(1), server, lane_rules(), clock)
}

/// Session key 2, whose randomness is all 6s.
fn signer() -> Signer {
    Signer::new(local_match::keypair(2), |bytes| bytes.fill(6))
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

/// The start of the match in slot 1 at sim tick 0, Lightyear tick 40, with a new chain.
const START: MatchStart = MatchStart {
    start_tick: 40,
    first: Tick::new(0),
    slot: PlayerSlot::new(1),
    chain: None,
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
    assert_eq!(
        granted.session_key,
        local_match::keypair(2).x_only_public_key().0
    );
    assert_eq!(
        delegation.main_key(),
        &local_match::keypair(1).x_only_public_key().0.serialize()
    );
    assert_eq!(
        (granted.seed_contribution, granted.expiration),
        ([6; 32], NOW + 86_400)
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

    // The match starts once: sim tick 0 is Lightyear tick 40, and the player's chain in slot 1
    // starts from the delegation's id.
    assert_eq!(state.start(START), Started::Playing { discarded: 0 });
    assert_eq!(state.start(START), Started::No);
    let clock = state.clock().unwrap();
    assert_eq!(
        [39, 40, 42].map(|tick| clock.sim_tick(NetTick(tick))),
        [None, Some(Tick::new(0)), Some(Tick::new(2))]
    );
    let playing = state.playing_mut().unwrap();
    assert_eq!(playing.session.id, offer.terms.session_id());
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
