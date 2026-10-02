use super::*;

const CREATED_AT: u64 = 1_700_000_000;

fn main_key() -> Keypair {
    let secret = secp256k1::SecretKey::from_byte_array(&[11; 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

fn terms() -> DelegationTerms {
    let secp = Secp256k1::new();
    let session = secp256k1::SecretKey::from_byte_array(&[21; 32]).unwrap();
    DelegationTerms {
        session_key: session.x_only_public_key(&secp).0,
        server_key: XOnlyPublicKey::from_byte_array(&[41; 32]).unwrap(),
        session_id: SessionId::new([31; 32]),
        seed_contribution: [51; 32],
        expiration: 1_700_086_400,
    }
}

fn signed() -> Delegation {
    Delegation::sign(
        &Secp256k1::new(),
        &main_key(),
        &terms(),
        CREATED_AT,
        &[0; 32],
    )
}

/// The JSON of an event of `kind` with `tags`, signed by the main key.
fn event(kind: u16, tags: Vec<Tag>) -> String {
    let keys = Keys::new(SecretKey::from(main_key().secret_key()));
    let unsigned = UnsignedEvent::new(
        keys.public_key(),
        Timestamp::from_secs(CREATED_AT),
        Kind::from_u16(kind),
        tags,
        "",
    );
    let id = unsigned.compute_id();
    let signature = keys.sign_schnorr_with_aux_rand(&Secp256k1::new(), id.as_bytes(), &[0; 32]);
    unsigned.add_signature(signature).unwrap().as_json()
}

/// The delegation's tags as `(name, value)`, with `change` applied.
fn tags(change: impl FnOnce(&mut Vec<Vec<String>>)) -> Vec<Tag> {
    let mut tags = vec![
        vec![
            DelegationTag::SessionKey.name().to_owned(),
            Bytes32::new(terms().session_key.serialize()).to_string(),
        ],
        vec![DelegationTag::ServerKey.name().to_owned(), "29".repeat(32)],
        vec![DelegationTag::SessionId.name().to_owned(), "1f".repeat(32)],
        vec![
            DelegationTag::SeedContribution.name().to_owned(),
            "33".repeat(32),
        ],
        vec![
            DelegationTag::Expiration.name().to_owned(),
            "1700086400".to_owned(),
        ],
    ];
    change(&mut tags);
    tags.into_iter()
        .map(|tag| Tag::parse(tag).unwrap())
        .collect()
}

#[test]
fn a_signed_delegation_reads_back_its_terms() {
    let delegation = signed();
    assert_eq!(delegation.terms(), &terms());
    assert_eq!(
        delegation.main_key(),
        &main_key().x_only_public_key().0.serialize()
    );

    // The event is of the delegation kind, with one tag per term in hex or decimal ([41; 32] is
    // 0x29 each, [31; 32] is 0x1f each, [51; 32] is 0x33 each) and no content; its id is the
    // chain root.
    let parsed = Event::from_json(delegation.json()).unwrap();
    assert_eq!(parsed.kind, Kind::from_u16(22_710));
    assert_eq!(parsed.created_at.as_secs(), CREATED_AT);
    let written: Vec<&[String]> = parsed.tags.iter().map(Tag::as_slice).collect();
    let expected = tags(|_| ());
    let expected: Vec<&[String]> = expected.iter().map(Tag::as_slice).collect();
    assert_eq!(written, expected);
    assert_eq!(parsed.content, "");
    assert_eq!(delegation.chain_root().as_bytes(), parsed.id.as_bytes());
    assert_eq!(Delegation::parse(delegation.json()), Ok(delegation.clone()));

    // The same tags built by hand give the same event.
    assert_eq!(event(KIND, tags(|_| ())), delegation.json());
}

#[test]
fn a_flawed_delegation_is_refused() {
    let json = signed().json().to_owned();
    // The signature's first hex digit, moved to its next digit: still hex, no longer the signature.
    let sig_at = json.find("\"sig\":\"").unwrap() + "\"sig\":\"".len();
    let mut forged = json.clone().into_bytes();
    forged[sig_at] = if forged[sig_at] == b'0' { b'1' } else { b'0' };
    let forged = String::from_utf8(forged).unwrap();

    let cases = [
        ("not JSON".to_owned(), DelegationError::NotEvent),
        ("{}".to_owned(), DelegationError::NotEvent),
        (
            json.replace("\"content\":\"\"", "\"content\":\"x\""),
            DelegationError::WrongId,
        ),
        (forged, DelegationError::BadSignature),
        (event(1, tags(|_| ())), DelegationError::WrongKind),
        (
            event(KIND, tags(|tags| drop(tags.remove(0)))),
            DelegationError::MissingTag(DelegationTag::SessionKey),
        ),
        (
            event(KIND, tags(|tags| drop(tags.remove(1)))),
            DelegationError::MissingTag(DelegationTag::ServerKey),
        ),
        (
            event(KIND, tags(|tags| drop(tags.remove(2)))),
            DelegationError::MissingTag(DelegationTag::SessionId),
        ),
        (
            event(KIND, tags(|tags| drop(tags.remove(3)))),
            DelegationError::MissingTag(DelegationTag::SeedContribution),
        ),
        (
            event(KIND, tags(|tags| drop(tags.remove(4)))),
            DelegationError::MissingTag(DelegationTag::Expiration),
        ),
        (
            event(KIND, tags(|tags| tags.push(tags[1].clone()))),
            DelegationError::RepeatedTag(DelegationTag::ServerKey),
        ),
        (
            event(KIND, tags(|tags| tags[4].push("1".to_owned()))),
            DelegationError::MalformedTag(DelegationTag::Expiration),
        ),
        (
            event(KIND, tags(|tags| tags[4][1] = "soon".to_owned())),
            DelegationError::MalformedTag(DelegationTag::Expiration),
        ),
        // 0x00…00 and 0x03…03 are not the x coordinate of a curve point.
        (
            event(KIND, tags(|tags| tags[0][1] = "00".repeat(32))),
            DelegationError::MalformedTag(DelegationTag::SessionKey),
        ),
        (
            event(KIND, tags(|tags| tags[1][1] = "03".repeat(32))),
            DelegationError::MalformedTag(DelegationTag::ServerKey),
        ),
        (
            event(KIND, tags(|tags| tags[1][1] = "29".repeat(31))),
            DelegationError::MalformedTag(DelegationTag::ServerKey),
        ),
        // Uppercase is not the one spelling Nostr writes.
        (
            event(KIND, tags(|tags| tags[2][1] = "1F".repeat(32))),
            DelegationError::MalformedTag(DelegationTag::SessionId),
        ),
        (
            event(KIND, tags(|tags| tags[3][1] = "33".repeat(33))),
            DelegationError::MalformedTag(DelegationTag::SeedContribution),
        ),
        (
            event(KIND, tags(|tags| tags[3].push("33".repeat(32)))),
            DelegationError::MalformedTag(DelegationTag::SeedContribution),
        ),
        (
            event(KIND, tags(|tags| tags.push(tags[3].clone()))),
            DelegationError::RepeatedTag(DelegationTag::SeedContribution),
        ),
    ];
    for (json, error) in cases {
        assert_eq!(Delegation::parse(&json), Err(error), "{json}");
    }

    // Other tags and a content change nothing but the id.
    let extra = event(
        KIND,
        tags(|tags| tags.push(vec!["t".to_owned(), "x".to_owned()])),
    );
    assert_eq!(Delegation::parse(&extra).unwrap().terms(), &terms());
}

#[test]
fn the_main_key_signs_the_seed_contribution() {
    // Another contribution in the JSON changes the event's hash, so the id no longer matches, and
    // with the id recomputed, the signature no longer holds.
    let json = signed().json().to_owned();
    let other = "34".repeat(32);
    let changed = json.replace(&"33".repeat(32), &other);
    assert_eq!(Delegation::parse(&changed), Err(DelegationError::WrongId));
    let signed = Event::from_json(&json).unwrap();
    let changed_tags = tags(|tags| tags[3][1] = other);
    let unsigned = UnsignedEvent::new(
        signed.pubkey,
        signed.created_at,
        signed.kind,
        changed_tags.clone(),
        "",
    );
    let resigned = Event::new(
        unsigned.compute_id(),
        signed.pubkey,
        signed.created_at,
        signed.kind,
        changed_tags,
        "",
        signed.sig,
    );
    assert_eq!(
        Delegation::parse(&resigned.as_json()),
        Err(DelegationError::BadSignature)
    );
}
