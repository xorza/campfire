use std::fmt::Write;

use nostr::event::{Event, Kind, Tag, UnsignedEvent};
use nostr::key::{Keys, SecretKey};
use nostr::types::Timestamp;
use secp256k1::{Keypair, Secp256k1, Signing, XOnlyPublicKey};

use crate::delegation::error::DelegationError;
use crate::input_hash::InputHash;
use crate::session_id::SessionId;

pub(crate) mod error;

/// The Nostr kind of a delegation. Ephemeral, so a relay sent one by mistake does not keep it.
const KIND: u16 = 22_710;
const SESSION_KEY: &str = "session_key";
const SERVER_KEY: &str = "server_key";
const SESSION_ID: &str = "session_id";
/// NIP-40's tag, in Unix seconds.
const EXPIRATION: &str = "expiration";

/// What a delegation grants: its session key signs for its main key in one session on one server,
/// until it expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegationTerms {
    pub session_key: XOnlyPublicKey,
    /// The server's x-only public key.
    pub server_key: [u8; 32],
    pub session_id: SessionId,
    /// Unix seconds.
    pub expiration: u64,
}

/// A session-key delegation: a Nostr event of the delegation kind, signed by the player's main key,
/// with one tag for each term. The log holds its JSON verbatim, so a verifier links every input
/// to the player's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegation {
    json: String,
    id: [u8; 32],
    main_key: [u8; 32],
    terms: DelegationTerms,
}

impl Delegation {
    /// `terms` signed by `main_key` at `created_at`, in Unix seconds, with BIP-340's auxiliary
    /// randomness `aux`.
    pub fn sign<C: Signing>(
        secp: &Secp256k1<C>,
        main_key: &Keypair,
        terms: &DelegationTerms,
        created_at: u64,
        aux: &[u8; 32],
    ) -> Delegation {
        let main_key = Keys::new(SecretKey::from(main_key.secret_key()));
        let tags = [
            Tag::custom(SESSION_KEY, [hex(&terms.session_key.serialize())]),
            Tag::custom(SERVER_KEY, [hex(&terms.server_key)]),
            Tag::custom(SESSION_ID, [hex(terms.session_id.as_bytes())]),
            Tag::custom(EXPIRATION, [terms.expiration.to_string()]),
        ];
        let unsigned = UnsignedEvent::new(
            main_key.public_key(),
            Timestamp::from_secs(created_at),
            Kind::from_u16(KIND),
            tags,
            "",
        );
        let id = unsigned.compute_id();
        let signature = main_key.sign_schnorr_with_aux_rand(secp, id.as_bytes(), aux);
        let event = unsigned
            .add_signature(signature)
            .expect("a fresh signature holds");
        Delegation::parse(&event.as_json()).expect("a signed delegation parses")
    }

    /// Reads a delegation from its event's JSON, checking the event's id and signature, its kind,
    /// and each term's tag. Other tags and the content are ignored.
    pub fn parse(json: &str) -> Result<Delegation, DelegationError> {
        let event = Event::from_json(json)
            .ok()
            .ok_or(DelegationError::NotEvent)?;
        if !event.verify_id() {
            return Err(DelegationError::WrongId);
        }
        if !event.verify_signature() {
            return Err(DelegationError::BadSignature);
        }
        if event.kind != Kind::from_u16(KIND) {
            return Err(DelegationError::WrongKind);
        }
        let session_key = unhex(tag(&event, SESSION_KEY)?)
            .and_then(|bytes| XOnlyPublicKey::from_byte_array(&bytes).ok())
            .ok_or(DelegationError::MalformedTag(SESSION_KEY))?;
        let server_key =
            unhex(tag(&event, SERVER_KEY)?).ok_or(DelegationError::MalformedTag(SERVER_KEY))?;
        let session_id =
            unhex(tag(&event, SESSION_ID)?).ok_or(DelegationError::MalformedTag(SESSION_ID))?;
        let expiration = tag(&event, EXPIRATION)?
            .parse()
            .ok()
            .ok_or(DelegationError::MalformedTag(EXPIRATION))?;
        Ok(Delegation {
            json: json.to_owned(),
            id: event.id.to_bytes(),
            main_key: event.pubkey.to_bytes(),
            terms: DelegationTerms {
                session_key,
                server_key,
                session_id: SessionId::new(session_id),
                expiration,
            },
        })
    }

    /// The event's JSON, as the player signed it.
    pub fn json(&self) -> &str {
        &self.json
    }

    /// What the player's first input links to: the event id, so the chain covers the delegation.
    pub const fn chain_root(&self) -> InputHash {
        InputHash::new(self.id)
    }

    /// The x-only public key of the player's Nostr identity.
    pub const fn main_key(&self) -> &[u8; 32] {
        &self.main_key
    }

    pub const fn terms(&self) -> &DelegationTerms {
        &self.terms
    }
}

/// The one value of the tag `name`.
fn tag<'a>(event: &'a Event, name: &'static str) -> Result<&'a str, DelegationError> {
    let mut found = None;
    for tag in event.tags.iter() {
        let [tag_name, values @ ..] = tag.as_slice() else {
            continue;
        };
        if tag_name != name {
            continue;
        }
        let [value] = values else {
            return Err(DelegationError::MalformedTag(name));
        };
        if found.replace(value.as_str()).is_some() {
            return Err(DelegationError::RepeatedTag(name));
        }
    }
    found.ok_or(DelegationError::MissingTag(name))
}

/// Lowercase hex, as Nostr writes keys and ids.
fn hex(bytes: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("a String takes any write");
    }
    hex
}

/// 32 bytes from exactly 64 lowercase hex digits, the one spelling `hex` writes.
fn unhex(hex: &str) -> Option<[u8; 32]> {
    let digits = hex.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let digit = |d: u8| match d {
        b'0'..=b'9' => Some(d - b'0'),
        b'a'..=b'f' => Some(d - b'a' + 10),
        _ => None,
    };
    let mut bytes = [0; 32];
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, &[high, low]) in bytes.iter_mut().zip(pairs) {
        *byte = digit(high)? << 4 | digit(low)?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests;
