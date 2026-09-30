use crate::input_hash::InputHash;
use crate::player_input::PlayerInput;
use crate::player_slot::PlayerSlot;
use crate::session_log::error::InputError;

/// A player's input chain: the hash its next input links to, and that input's seq. The player
/// extends it with each input it sends; the log accepts each input it records against its own
/// copy, so both sides move on by the same rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputChain {
    slot: PlayerSlot,
    head: InputHash,
    next_seq: u64,
}

impl InputChain {
    /// A chain with no input yet: the first one links to `root`.
    pub const fn new(slot: PlayerSlot, root: InputHash) -> InputChain {
        InputChain {
            slot,
            head: root,
            next_seq: 0,
        }
    }

    pub const fn slot(&self) -> PlayerSlot {
        self.slot
    }

    /// The player's next input, stamped for `stamp`; the chain moves on to it.
    pub fn extend<'a>(&mut self, stamp: u64, payload: &'a [u8]) -> PlayerInput<'a> {
        let input = PlayerInput {
            slot: self.slot,
            seq: self.next_seq,
            stamp,
            previous: self.head,
            payload,
        };
        self.move_on(&input);
        input
    }

    /// Moves on to `input` when it is the chain's next one; unchanged otherwise.
    pub(crate) fn accept(&mut self, input: &PlayerInput<'_>) -> Result<(), InputError> {
        debug_assert_eq!(
            input.slot, self.slot,
            "a chain accepts its own player's inputs"
        );
        if input.previous != self.head {
            return Err(InputError::BrokenLink);
        }
        if input.seq != self.next_seq {
            return Err(InputError::WrongSeq {
                expected: self.next_seq,
            });
        }
        self.move_on(input);
        Ok(())
    }

    fn move_on(&mut self, input: &PlayerInput<'_>) {
        self.head = input.hash();
        self.next_seq = self.next_seq.checked_add(1).expect("input seq exhausted");
    }
}

#[cfg(feature = "bench")]
pub(crate) mod bench;
