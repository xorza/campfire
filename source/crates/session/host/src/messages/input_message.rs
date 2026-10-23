use campfire_common::Tick;
use campfire_protocol::Signature;
use serde::{Deserialize, Serialize};

/// A packet of player inputs as the client sends it, signed once over the chain head after the
/// last. The server knows the sender's player slot from the connection, so the message does not
/// carry it. The payloads sit end to end in one buffer, each frame giving the length of its own.
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
    /// A message of `inputs`, each its stamp and payload, with `signature` over the chain head
    /// after the last. Its buffers take their exact sizes first, from a pass over the inputs.
    pub fn new<'a, I>(inputs: I, signature: Signature) -> Self
    where
        I: IntoIterator<Item = (Tick, &'a [u8])>,
        I::IntoIter: ExactSizeIterator + Clone,
    {
        let inputs = inputs.into_iter();
        let bytes = inputs.clone().map(|(_, payload)| payload.len()).sum();
        let mut message = Self {
            frames: Vec::with_capacity(inputs.len()),
            payloads: Vec::with_capacity(bytes),
            signature,
        };
        for (stamp, payload) in inputs {
            message.frames.push(InputFrame {
                stamp,
                payload_len: u32::try_from(payload.len()).expect("a payload fits u32"),
            });
            message.payloads.extend_from_slice(payload);
        }
        message
    }

    /// The inputs, each its stamp and payload; `None` when the frames' lengths do not add up to
    /// the payload bytes, which only a broken client sends.
    pub fn inputs(&self) -> Option<impl Iterator<Item = (Tick, &[u8])> + Clone> {
        let total = self.frames.iter().try_fold(0_usize, |total, frame| {
            total.checked_add(usize::try_from(frame.payload_len).expect("a u32 fits usize"))
        })?;
        if total != self.payloads.len() {
            return None;
        }
        let mut start = 0;
        Some(self.frames.iter().map(move |frame| {
            let end = start + usize::try_from(frame.payload_len).expect("a u32 fits usize");
            let payload = &self.payloads[start..end];
            start = end;
            (frame.stamp, payload)
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
        let sent = [
            (Tick::new(7), &b"ab"[..]),
            (Tick::new(7), b""),
            (Tick::new(8), b"cde"),
        ];
        let message = InputMessage::new(sent, signature);
        assert_eq!(message.payloads, b"abcde");
        // Three frames and 2 + 0 + 3 bytes, each buffer at its exact size, which
        // `Vec::with_capacity` gives.
        let capacities = (message.frames.capacity(), message.payloads.capacity());
        assert_eq!(capacities, (3, 5));
        let received: Vec<_> = message.inputs().unwrap().collect();
        assert_eq!(received, sent);
        assert_eq!(message.signature(), &signature);

        // Lengths short of, past, or overflowing the payload bytes give no inputs.
        for lens in [[2, 0, 2], [2, 1, 3], [u32::MAX, u32::MAX, u32::MAX]] {
            let mut broken = message.clone();
            for (frame, len) in broken.frames.iter_mut().zip(lens) {
                frame.payload_len = len;
            }
            assert!(broken.inputs().is_none(), "{lens:?}");
        }
    }
}
