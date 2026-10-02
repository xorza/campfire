use campfire_common::{PlayerSlot, Tick};
use campfire_protocol::{PlayerInput, Signature};
use serde::{Deserialize, Serialize};

/// A packet of player inputs as the client sends it, signed once over the chain head after the
/// last. The server knows the sender's slot from the connection, so the message does not carry
/// it. The payloads sit end to end in one buffer, each frame giving the length of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputMessage {
    frames: Vec<InputFrame>,
    payloads: Vec<u8>,
    signature: Signature,
}

/// One input of a message, without its payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct InputFrame {
    stamp: Tick,
    payload_len: u32,
}

impl InputMessage {
    /// A message of `inputs`, with `signature` over the chain head after the last.
    pub fn new<'a>(
        inputs: impl IntoIterator<Item = PlayerInput<'a>>,
        signature: Signature,
    ) -> InputMessage {
        let mut message = InputMessage {
            frames: Vec::new(),
            payloads: Vec::new(),
            signature,
        };
        for input in inputs {
            message.frames.push(InputFrame {
                stamp: input.stamp,
                payload_len: u32::try_from(input.payload.len()).expect("a payload fits u32"),
            });
            message.payloads.extend_from_slice(input.payload);
        }
        message
    }

    /// The inputs of the player in `slot`; `None` when the frames' lengths do not add up to the
    /// payload bytes, which only a broken client sends.
    pub fn inputs(
        &self,
        slot: PlayerSlot,
    ) -> Option<impl Iterator<Item = PlayerInput<'_>> + Clone> {
        let total = self.frames.iter().try_fold(0_usize, |total, frame| {
            total.checked_add(frame.payload_len as usize)
        })?;
        if total != self.payloads.len() {
            return None;
        }
        let mut start = 0;
        Some(self.frames.iter().map(move |frame| {
            let end = start + frame.payload_len as usize;
            let payload = &self.payloads[start..end];
            start = end;
            PlayerInput {
                slot,
                stamp: frame.stamp,
                payload,
            }
        }))
    }

    pub const fn signature(&self) -> &Signature {
        &self.signature
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_gives_back_its_inputs_or_none() {
        let signature = Signature::from_bytes([3; 64]);
        let slot = PlayerSlot::new(1);
        let input = |stamp, payload| PlayerInput {
            slot,
            stamp,
            payload,
        };
        let sent = [
            input(Tick::new(7), &b"ab"[..]),
            input(Tick::new(7), b""),
            input(Tick::new(8), b"cde"),
        ];
        let message = InputMessage::new(sent, signature);
        assert_eq!(message.payloads, b"abcde");
        let received: Vec<_> = message.inputs(slot).unwrap().collect();
        assert_eq!(received, sent);
        assert_eq!(message.signature(), &signature);

        // Lengths short of, past, or overflowing the payload bytes give no inputs.
        for lens in [[2, 0, 2], [2, 1, 3], [u32::MAX, u32::MAX, u32::MAX]] {
            let mut broken = message.clone();
            for (frame, len) in broken.frames.iter_mut().zip(lens) {
                frame.payload_len = len;
            }
            assert!(broken.inputs(slot).is_none(), "{lens:?}");
        }
    }
}
