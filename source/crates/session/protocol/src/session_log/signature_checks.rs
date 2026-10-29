use std::collections::VecDeque;
use std::ops::Range;

use secp256k1::XOnlyPublicKey;

use crate::signature::Signature;
use crate::signed::{self, Signed};
use crate::verifier::Verifier;

/// How the log checks the signatures of what it takes: each at once, as a live log and a decode
/// do, or, as a restore replays its server's own journal, each kept until the replay knows its
/// base, the latest checkpoint whose record it holds, so the signatures before the base, which the
/// live log forgets, are trusted and those after it checked at the end (08-sessions.md, A restore
/// trusts its journal before its base).
#[derive(Debug)]
pub(super) struct SignatureChecks {
    verifier: Verifier,
    /// The scratch a message is written into.
    message: Vec<u8>,
    deferred: Option<DeferredChecks>,
}

/// The checks a replay keeps: each message, in one buffer, and each check of one, and where each
/// checkpoint begun and not yet recorded starts among the checks.
#[derive(Debug, Default)]
struct DeferredChecks {
    messages: Vec<u8>,
    checks: Vec<DeferredCheck>,
    boundaries: VecDeque<usize>,
    /// The journal record the checks taken now are of.
    record: u64,
}

/// A signature to check: whose key, over which bytes of the messages, of which journal record.
#[derive(Debug)]
struct DeferredCheck {
    signature: Signature,
    key: XOnlyPublicKey,
    message: Range<usize>,
    record: u64,
}

impl SignatureChecks {
    pub(super) fn new() -> Self {
        Self {
            verifier: Verifier::new(),
            message: Vec::new(),
            deferred: None,
        }
    }

    /// Whether `signature` is `key`'s over `record`; true while the checks are deferred, which
    /// keep it for `finish`.
    pub(super) fn verifies(
        &mut self,
        signature: &Signature,
        key: &XOnlyPublicKey,
        record: &impl Signed,
    ) -> bool {
        let Some(deferred) = &mut self.deferred else {
            return self.verifier.verifies(signature, key, record);
        };
        signed::write_message(record, &mut self.message);
        let start = deferred.messages.len();
        deferred.messages.extend_from_slice(&self.message);
        deferred.checks.push(DeferredCheck {
            signature: *signature,
            key: *key,
            message: start..deferred.messages.len(),
            record: deferred.record,
        });
        true
    }

    /// Defers each check from now on, until `finish`.
    pub(super) fn defer(&mut self) {
        debug_assert!(self.deferred.is_none(), "a replay defers its checks once");
        self.deferred = Some(DeferredChecks::default());
    }

    /// Notes that the checks taken from now on are of journal record `record`.
    pub(super) const fn at_record(&mut self, record: u64) {
        if let Some(deferred) = &mut self.deferred {
            deferred.record = record;
        }
    }

    /// Notes a checkpoint begun at the boundary before the next tick: the checks taken from now
    /// on are of its segment.
    pub(super) fn begin_checkpoint(&mut self) {
        if let Some(deferred) = &mut self.deferred {
            deferred.boundaries.push_back(deferred.checks.len());
        }
    }

    /// Trusts each check before the boundary of the oldest checkpoint begun, whose record the
    /// log took: a base the live log forgets all before. A record of no begin the replay saw
    /// trusts nothing.
    pub(super) fn record_checkpoint(&mut self) {
        let Some(deferred) = &mut self.deferred else {
            return;
        };
        let Some(boundary) = deferred.boundaries.pop_front() else {
            return;
        };
        let message_start = deferred
            .checks
            .get(boundary)
            .map_or(deferred.messages.len(), |check| check.message.start);
        deferred.checks.drain(..boundary);
        deferred.messages.drain(..message_start);
        for check in &mut deferred.checks {
            check.message = check.message.start - message_start..check.message.end - message_start;
        }
        for later in &mut deferred.boundaries {
            *later -= boundary;
        }
    }

    /// Trusts every check kept, as a load goes back to a checkpoint whose record the log holds,
    /// and drops the checkpoints begun after it.
    pub(super) fn load(&mut self) {
        if let Some(deferred) = &mut self.deferred {
            *deferred = DeferredChecks {
                record: deferred.record,
                ..DeferredChecks::default()
            };
        }
    }

    /// Checks each signature kept, and checks each at once from now on; the journal record of the
    /// first that fails, if one does.
    pub(super) fn finish(&mut self) -> Result<(), u64> {
        let Some(deferred) = self.deferred.take() else {
            return Ok(());
        };
        for check in &deferred.checks {
            let message = &deferred.messages[check.message.clone()];
            if !self
                .verifier
                .verifies_message(&check.signature, &check.key, message)
            {
                return Err(check.record);
            }
        }
        Ok(())
    }
}
