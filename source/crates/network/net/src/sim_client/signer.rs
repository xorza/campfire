use bevy_ecs::resource::Resource;
use campfire_common::ChannelBinding;
use campfire_protocol::{
    ConnectChallenge, Delegation, DelegationTerms, InputChain, RandomKey, SessionId, Signature,
};
use secp256k1::{Keypair, Secp256k1, SignOnly, XOnlyPublicKey};

use crate::os::Entropy;

/// The player's session key, which signs their join answer and every input message, with the
/// context its signatures take and what fills their randomness.
#[derive(Resource, Debug)]
pub(crate) struct Signer {
    key: Keypair,
    secp: Secp256k1<SignOnly>,
    /// Fills a seed contribution or BIP-340's auxiliary randomness.
    entropy: Entropy,
}

impl Signer {
    pub(crate) fn new(key: Keypair, entropy: Entropy) -> Signer {
        Signer {
            key,
            secp: Secp256k1::signing_only(),
            entropy,
        }
    }

    /// Takes a new session key, from fresh randomness, as a player renews their delegation.
    pub(crate) fn renew(&mut self) {
        self.key = RandomKey::generate(|bytes| self.entropy.fill(bytes));
    }

    pub(crate) fn random(&self) -> [u8; 32] {
        let mut bytes = [0; 32];
        self.entropy.fill(&mut bytes);
        bytes
    }

    /// `granted` signed by `main_key` at `now`, in Unix seconds.
    pub(crate) fn delegate(
        &self,
        main_key: &Keypair,
        granted: &DelegationTerms,
        now: u64,
    ) -> Delegation {
        Delegation::sign(&self.secp, main_key, granted, now, &self.random())
    }

    /// The session key's answer to `challenge`, over `binding`, its link's TLS session's.
    pub(crate) fn answer(
        &self,
        challenge: ConnectChallenge,
        binding: &ChannelBinding,
    ) -> Signature {
        challenge.answer(&self.secp, &self.key, binding, &self.random())
    }

    /// The session key's signature over the head of `chain` in the session of `session_id`.
    pub(crate) fn sign(&self, chain: &InputChain, session_id: SessionId) -> Signature {
        chain.sign(&self.secp, &self.key, session_id, &self.random())
    }

    pub(crate) fn public_key(&self) -> XOnlyPublicKey {
        self.key.x_only_public_key().0
    }
}
