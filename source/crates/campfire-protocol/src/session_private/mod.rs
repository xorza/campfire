use serde::{Deserialize, Serialize};

use crate::seed_chain::SeedChain;
use crate::session_private::error::SessionPrivateError;
use crate::session_terms::SessionTerms;

pub(crate) mod error;

/// Starts every private record and states its version, so other bytes are refused at once.
const PRIVATE_TAG: &[u8] = b"campfire/session-private/v1";

/// What a server keeps of a session that no one else sees, written once before its first offer:
/// the seed chain, whose root reveals every segment's seed, and the session's terms, so a
/// restart restores the session it opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPrivate {
    pub seed_chain: SeedChain,
    pub terms: SessionTerms,
}

impl SessionPrivate {
    /// The file's bytes: the tag, then the record in postcard.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = PRIVATE_TAG.to_vec();
        postcard::to_io(self, &mut bytes).expect("postcard into a Vec cannot fail");
        bytes
    }

    /// The record of a file's `bytes`; an error for bytes that `encode` did not write.
    pub fn decode(bytes: &[u8]) -> Result<SessionPrivate, SessionPrivateError> {
        let rest = bytes
            .strip_prefix(PRIVATE_TAG)
            .ok_or(SessionPrivateError::NotPrivate)?;
        let (private, rest) = postcard::take_from_bytes::<SessionPrivate>(rest)
            .map_err(SessionPrivateError::Malformed)?;
        if !rest.is_empty() {
            return Err(SessionPrivateError::Trailing);
        }
        Ok(private)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use campfire_common::{Fingerprint, Ticks};
    use secp256k1::{Keypair, Secp256k1, SecretKey};

    use super::*;
    use crate::slot_plan::SlotPlan;

    #[test]
    fn a_private_record_round_trips_and_other_bytes_are_refused() {
        let secret = SecretKey::from_byte_array(&[8; 32]).unwrap();
        let key = Keypair::from_secret_key(&Secp256k1::new(), &secret);
        let seed_chain = SeedChain::new([9; 32], NonZeroU32::new(1024).unwrap());
        let private = SessionPrivate {
            seed_chain,
            terms: SessionTerms {
                server_key: key.x_only_public_key().0,
                tick_hz: NonZeroU32::new(30).unwrap(),
                max_input_delay: Ticks::new(3),
                max_input_lead: Ticks::new(3),
                max_payload_len: 64,
                max_inputs_per_tick: 4,
                seed_commitment: seed_chain.commitment(),
                release: "0.1.0".to_owned(),
                mode: Fingerprint::new([1; 32]),
                dependencies: vec![Fingerprint::new([2; 32])],
                slots: vec![SlotPlan::Player, SlotPlan::Open],
            },
        };
        let bytes = private.encode();
        assert_eq!(SessionPrivate::decode(&bytes), Ok(private));
        assert_eq!(
            SessionPrivate::decode(&bytes[1..]),
            Err(SessionPrivateError::NotPrivate)
        );
        assert!(matches!(
            SessionPrivate::decode(&bytes[..bytes.len() - 1]),
            Err(SessionPrivateError::Malformed(_))
        ));
        assert_eq!(
            SessionPrivate::decode(&[bytes.as_slice(), &[0]].concat()),
            Err(SessionPrivateError::Trailing)
        );
    }
}
