use std::num::NonZeroU32;
use std::path::Path;

use campfire_math::{PlayerSlot, Tick};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{Secp256k1, SecretKey, XOnlyPublicKey};
use campfire_protocol::{CertificateHash, ConnectChallenge, Delegation, Fingerprint, SeedChain};
use campfire_runner::{InputRules, TermsError};

use super::*;

const NOW: u64 = 1_700_000_000;
const TICK_HZ: NonZeroU32 = NonZeroU32::new(30).unwrap();

type Change = fn(&mut SessionTerms);

fn keypair(byte: u8) -> Keypair {
    let secret = SecretKey::from_byte_array(&[byte; 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// The rules of the lane mode, which runs at 30 Hz only.
fn lane_rules() -> SessionRules {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/test/modes/lane"
    );
    SessionRules::of(&ModePackages::from_dir(Path::new(dir)).unwrap())
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
    JoinState::new(keypair(1), server, lane_rules(), || NOW)
}

/// Session key 2, whose randomness is all 6s.
fn signer() -> Signer {
    Signer::new(keypair(2), |bytes| bytes.fill(6))
}

/// Terms the client can play, with `change` applied.
fn offer(change: impl FnOnce(&mut SessionTerms)) -> Offer {
    let mut terms = lane_rules()
        .terms(
            x_only(8),
            SeedChain::new([7; 32], NonZeroU32::MIN).commitment(),
            TICK_HZ,
            InputRules::LAN,
        )
        .unwrap();
    change(&mut terms);
    Offer {
        terms,
        challenge: ConnectChallenge::new([9; 32]),
    }
}

#[test]
fn a_client_joins_only_the_session_its_server_offers_and_it_can_play() {
    let signer = signer();
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
            state.answer(&offer(change), &signer),
            Some(Err(mismatch.clone()))
        );
        assert_eq!(state.refusal(), Some(&mismatch));
        // A refused client answers no later offer, and starts no match.
        assert_eq!(state.answer(&offer(|_| {}), &signer), None);
        assert!(!state.start(MatchStart {
            start_tick: 40,
            slot: PlayerSlot::new(1)
        }));
    }

    // A fitting offer: the delegation names the offered session, lets session key 2 sign until a
    // day after now, and carries the contribution the entropy gave; the answer passes the
    // server's check.
    let mut state = waiting(TICK_HZ);
    let start = MatchStart {
        start_tick: 40,
        slot: PlayerSlot::new(1),
    };
    assert!(!state.start(start), "no match starts before the answer");
    let offer = offer(|_| {});
    let join = state.answer(&offer, &signer).unwrap().unwrap();
    assert_eq!(
        state.answer(&offer, &signer),
        None,
        "the client answers once"
    );
    let delegation = Delegation::parse(&join.delegation).unwrap();
    let granted = delegation.terms();
    assert_eq!(granted.session_id, offer.terms.session_id());
    assert_eq!(granted.session_key, keypair(2).x_only_public_key().0);
    assert_eq!(
        delegation.main_key(),
        &keypair(1).x_only_public_key().0.serialize()
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
    assert!(state.start(start));
    assert!(!state.start(start));
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
