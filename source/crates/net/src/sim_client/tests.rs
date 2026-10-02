use std::num::NonZeroU32;

use campfire_capabilities::{Bounds, CapabilitySet, Metric};
use campfire_math::Ticks;
use campfire_package::RELEASE;
use campfire_protocol::secp256k1::SecretKey;
use campfire_protocol::{CertificateHash, ConnectChallenge, Fingerprint, SeedChain, SessionTerms};

use super::*;

const NOW: u64 = 1_700_000_000;

fn keypair(byte: u8) -> Keypair {
    let secret = SecretKey::from_byte_array(&[byte; 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

fn client() -> SimClient {
    SimClient {
        main_key: keypair(1),
        session_key: keypair(2),
        server: ServerPin {
            key: [8; 32],
            certificate: CertificateHash::new([3; 32]),
        },
        mode: ClientMode {
            tick_hz: NonZeroU32::new(30).unwrap(),
            mode: Fingerprint::new([5; 32]),
            dependencies: vec![Fingerprint::new([4; 32])],
            capabilities: CapabilitySet::new(&[]).unwrap(),
            metric: Metric::Planar,
            bounds: Bounds::WORLD,
            pathing: None,
            walkers: Vec::new(),
            life: None,
        },
        clock: || NOW,
        entropy: |bytes| bytes.fill(6),
    }
}

/// Terms the client can play, with `change` applied.
fn offer(change: impl FnOnce(&mut SessionTerms)) -> Offer {
    let mut terms = SessionTerms {
        server_key: [8; 32],
        tick_hz: NonZeroU32::new(30).unwrap(),
        max_input_delay: Ticks::new(10),
        max_input_lead: Ticks::new(30),
        max_payload_len: 64,
        max_inputs_per_tick: 4,
        seed_commitment: SeedChain::new([7; 32], NonZeroU32::MIN).commitment(),
        release: RELEASE.to_owned(),
        mode: Fingerprint::new([5; 32]),
        dependencies: vec![Fingerprint::new([4; 32])],
    };
    change(&mut terms);
    Offer {
        terms,
        challenge: ConnectChallenge::new([9; 32]),
    }
}

fn sent() -> SentInputs {
    SentInputs {
        client: client(),
        secp: Secp256k1::signing_only(),
        session: None,
        chain: None,
        inputs: Vec::new(),
        payloads: Vec::new(),
    }
}

#[test]
fn a_client_joins_only_the_session_its_server_offers_and_it_can_play() {
    for (change, mismatch) in [
        (
            (|terms: &mut SessionTerms| terms.server_key = [9; 32]) as fn(&mut SessionTerms),
            TermsMismatch::OtherServer,
        ),
        (
            |terms| terms.release = "0.0.9".to_owned(),
            TermsMismatch::OtherRelease,
        ),
        (
            |terms| terms.mode = Fingerprint::new([0; 32]),
            TermsMismatch::OtherMode,
        ),
        (
            |terms| terms.dependencies.clear(),
            TermsMismatch::OtherDependencies,
        ),
        (
            |terms| terms.tick_hz = NonZeroU32::new(60).unwrap(),
            TermsMismatch::OtherTickRate,
        ),
    ] {
        let mut sent = sent();
        assert_eq!(sent.join(&offer(change)), Err(mismatch));
        assert!(sent.session.is_none(), "{mismatch:?}");
    }

    // A fitting offer: the delegation names the offered session, lets session key 2 sign until a
    // day after now, and carries the contribution the entropy gave; the answer passes the
    // server's check, and the player's chain will start from the delegation's id.
    let mut sent = sent();
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
