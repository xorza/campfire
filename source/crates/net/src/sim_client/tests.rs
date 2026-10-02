use std::num::NonZeroU32;
use std::path::Path;

use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{SecretKey, XOnlyPublicKey};
use campfire_protocol::{CertificateHash, ConnectChallenge, Fingerprint, SeedChain, SessionTerms};
use campfire_runner::{InputRules, SessionRules, TermsError};

use super::*;

const NOW: u64 = 1_700_000_000;

fn keypair(byte: u8) -> Keypair {
    let secret = SecretKey::from_byte_array(&[byte; 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// The lane mode, which runs at 30 Hz only.
fn lane_mode() -> ModePackages {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/test/modes/lane"
    );
    ModePackages::from_dir(Path::new(dir)).unwrap()
}

/// An x-only key of the bytes `[byte; 32]`, which must be the x of a point on the curve.
fn x_only(byte: u8) -> XOnlyPublicKey {
    XOnlyPublicKey::from_byte_array(&[byte; 32]).unwrap()
}

type Change = fn(&mut SessionTerms);

const TICK_HZ: NonZeroU32 = NonZeroU32::new(30).unwrap();

/// A client of the lane mode that means to reach the server of key 8 at `tick_hz`.
fn client(tick_hz: NonZeroU32) -> SimClient {
    SimClient {
        main_key: keypair(1),
        session_key: keypair(2),
        server: ServerPin {
            key: x_only(8),
            certificate: CertificateHash::new([3; 32]),
            tick_hz,
        },
        mode: ClientMode::of(&lane_mode()),
        clock: || NOW,
        entropy: |bytes| bytes.fill(6),
    }
}

/// Terms the client can play, with `change` applied.
fn offer(change: impl FnOnce(&mut SessionTerms)) -> Offer {
    let mut terms = SessionRules::of(&lane_mode())
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

fn sent(tick_hz: NonZeroU32) -> SentInputs {
    SentInputs {
        client: client(tick_hz),
        secp: Secp256k1::signing_only(),
        session: None,
        chain: None,
        inputs: Vec::new(),
        payloads: Vec::new(),
    }
}

#[test]
fn a_client_joins_only_the_session_its_server_offers_and_it_can_play() {
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
        let mut sent = sent(pinned);
        assert_eq!(sent.join(&offer(change)), Err(mismatch.clone()));
        assert!(sent.session.is_none(), "{mismatch:?}");
    }

    // A fitting offer: the delegation names the offered session, lets session key 2 sign until a
    // day after now, and carries the contribution the entropy gave; the answer passes the
    // server's check, and the player's chain will start from the delegation's id.
    let mut sent = sent(TICK_HZ);
    let offer = offer(|_| {});
    let join = sent.join(&offer).unwrap();
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
    let session = sent.session.unwrap();
    assert_eq!(
        (session.id, session.chain_root),
        (offer.terms.session_id(), delegation.chain_root())
    );
}
