use nostr::event::{Event, Kind, Tag, UnsignedEvent};
use nostr::key::{Keys, SecretKey};
use nostr::types::Timestamp;
use secp256k1::{Keypair, Secp256k1, Signing, XOnlyPublicKey};

use crate::delegation::delegation_tag::DelegationTag;
use crate::delegation::error::DelegationError;
use crate::input_hash::InputHash;
use crate::session_id::SessionId;
use campfire_math::Bytes32;

pub(crate) mod delegation_tag;
pub(crate) mod error;

/// The Nostr kind of a delegation. Ephemeral, so a relay sent one by mistake does not keep it.
const KIND: u16 = 22_710;

/// What a delegation grants: its session key signs for its main key in one session on one server,
/// until it expires. It also carries the player's seed contribution, so the main key signs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegationTerms {
    pub session_key: XOnlyPublicKey,
    /// The server's x-only public key.
    pub server_key: [u8; 32],
    pub session_id: SessionId,
    /// The player's random share of every segment's seed, chosen after the session id fixes
    /// the server's seed commitment, so no one can choose it with the seed in view.
    pub seed_contribution: [u8; 32],
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
            custom(
                DelegationTag::SessionKey,
                Bytes32::new(terms.session_key.serialize()).to_string(),
            ),
            custom(
                DelegationTag::ServerKey,
                Bytes32::new(terms.server_key).to_string(),
            ),
            custom(
                DelegationTag::SessionId,
                Bytes32::new(*terms.session_id.as_bytes()).to_string(),
            ),
            custom(
                DelegationTag::SeedContribution,
                Bytes32::new(terms.seed_contribution).to_string(),
            ),
            custom(DelegationTag::Expiration, terms.expiration.to_string()),
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
        let bytes = |name: DelegationTag| -> Result<[u8; 32], DelegationError> {
            let parsed = tag(&event, name)?.parse::<Bytes32>().ok();
            parsed
                .map(Bytes32::get)
                .ok_or(DelegationError::MalformedTag(name))
        };
        let session_key = XOnlyPublicKey::from_byte_array(&bytes(DelegationTag::SessionKey)?)
            .ok()
            .ok_or(DelegationError::MalformedTag(DelegationTag::SessionKey))?;
        let server_key = bytes(DelegationTag::ServerKey)?;
        let session_id = bytes(DelegationTag::SessionId)?;
        let seed_contribution = bytes(DelegationTag::SeedContribution)?;
        let expiration = tag(&event, DelegationTag::Expiration)?
            .parse()
            .ok()
            .ok_or(DelegationError::MalformedTag(DelegationTag::Expiration))?;
        Ok(Delegation {
            json: json.to_owned(),
            id: event.id.to_bytes(),
            main_key: event.pubkey.to_bytes(),
            terms: DelegationTerms {
                session_key,
                server_key,
                session_id: SessionId::new(session_id),
                seed_contribution,
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

/// The tag `delegation_tag` with its one value.
fn custom(delegation_tag: DelegationTag, value: String) -> Tag {
    Tag::custom(delegation_tag.name(), [value])
}

/// The one value of the tag `delegation_tag`.
fn tag(event: &Event, delegation_tag: DelegationTag) -> Result<&str, DelegationError> {
    let name = delegation_tag.name();
    let mut found = None;
    for tag in event.tags.iter() {
        let [tag_name, values @ ..] = tag.as_slice() else {
            continue;
        };
        if tag_name != name {
            continue;
        }
        let [value] = values else {
            return Err(DelegationError::MalformedTag(delegation_tag));
        };
        if found.replace(value.as_str()).is_some() {
            return Err(DelegationError::RepeatedTag(delegation_tag));
        }
    }
    found.ok_or(DelegationError::MissingTag(delegation_tag))
}

#[cfg(test)]
mod tests;
