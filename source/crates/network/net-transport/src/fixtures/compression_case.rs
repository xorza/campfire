use crate::fixtures::stepper_counts::summed_transport_counts;
use bytes::Bytes;
use campfire_net_stepper::StepperSetup;
use campfire_net_stepper::{StepOrder, Stepper};

use crate::fixtures::protocol::{Channel1, channel_id};
use crate::fixtures::transport_stack::TransportStack;
use crate::transport::Transport;
use campfire_net_channels::internals::CompressionMode;
use campfire_net_channels::internals::PayloadPattern;
use campfire_net_channels::internals::TransportCounts;

/// A burst of raw messages from the server to one client, all queued before one frame.
#[derive(Clone, Copy, Debug)]
pub struct CompressionCase {
    pub name: &'static str,
    pub pattern: PayloadPattern,
    pub message_len: usize,
    pub messages: usize,
}

impl CompressionCase {
    pub const ALL: [Self; 4] = [
        Self {
            name: "repeated_64b_128msg",
            pattern: PayloadPattern::Repeated,
            message_len: 64,
            messages: 128,
        },
        Self {
            name: "structured_128b_128msg",
            pattern: PayloadPattern::Structured,
            message_len: 128,
            messages: 128,
        },
        Self {
            name: "random_ascii_128b_128msg",
            pattern: PayloadPattern::RandomAscii,
            message_len: 128,
            messages: 128,
        },
        Self {
            name: "repeated_1024b_16msg",
            pattern: PayloadPattern::Repeated,
            message_len: 1024,
            messages: 16,
        },
    ];

    /// Sends the case's messages and counts one server-first frame.
    pub fn run(self, mode: CompressionMode) -> TransportCounts {
        self.prepare(mode).run()
    }

    /// Queues the case's messages on a fresh stepper whose transports use `mode`.
    pub fn prepare(self, mode: CompressionMode) -> CompressionRun {
        let mut stepper = Stepper::new(TransportStack, StepperSetup::single());
        for client in 0..stepper.client_count() {
            stepper
                .client_mut(client)
                .get_mut::<Transport>()
                .unwrap()
                .set_compression(mode.setup());
            stepper
                .client_of_mut(client)
                .get_mut::<Transport>()
                .unwrap()
                .set_compression(mode.setup());
        }
        let channel_id = channel_id::<Channel1>(stepper.server_app());
        for message_index in 0..self.messages {
            let payload = self.pattern.payload(self.message_len, message_index);
            stepper
                .client_of_mut(0)
                .get_mut::<Transport>()
                .unwrap()
                .send(channel_id, Bytes::from(payload.into_bytes()), 1.0)
                .unwrap();
        }
        CompressionRun { stepper }
    }
}

/// A case's messages, queued and not yet sent.
#[derive(Debug)]
pub struct CompressionRun {
    stepper: Stepper<TransportStack>,
}

impl CompressionRun {
    /// Runs one server-first frame and counts what its transports did.
    pub fn run(&mut self) -> TransportCounts {
        let before = summed_transport_counts(&mut self.stepper);
        self.frame();
        summed_transport_counts(&mut self.stepper) - before
    }

    /// Runs one server-first frame.
    pub fn frame(&mut self) {
        let interval = self.stepper.frame_interval();
        self.stepper.step(1, interval, StepOrder::ServerFirst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counts of a run that sent `[bytes, packets]` and received `received_bytes`,
    /// uncompressed.
    const fn counts(sent: [u64; 2], received_bytes: u64) -> TransportCounts {
        let [sent_bytes, sent_packets] = sent;
        TransportCounts {
            sent_bytes,
            sent_packets,
            resent_messages: 0,
            received_bytes,
            compression_saved_bytes: 0,
            compression_abandoned: 0,
        }
    }

    /// `counts` of a run whose compression saved `saved` bytes and abandoned `abandoned` packets.
    const fn compressed(counts: TransportCounts, saved: u64, abandoned: u64) -> TransportCounts {
        TransportCounts {
            compression_saved_bytes: saved,
            compression_abandoned: abandoned,
            ..counts
        }
    }

    /// What compression saves on the wire, for each burst of [`CompressionCase::ALL`] of raw
    /// messages on the transport stack, in its one frame. The stepper is deterministic, so a byte
    /// count is exact, and a change to the codec or to the packing shows as a diff of these values.
    ///
    /// The plain packet session carries 1,190 bytes a packet, and its initial window of 12,000
    /// bytes takes 10 packets of 1,177 and 10 of their packet session's bytes, or 11 of 1,033. A
    /// packet starts with a long tick of 5 and a batch's channel id and count; a message of 64
    /// bytes takes 66 with its length, one of 128 takes 130, and one of 1,024 takes 1,026. So 128
    /// messages of 64 are 7 packets of 17 and one of 9: 7 · (7 + 17 · 66) + 7 + 9 · 66 = 8,504. The
    /// server reads 5 bytes more: the client's heartbeat of the frame before, a long tick.
    #[test]
    fn each_case_sends_exact_bytes_in_each_mode() {
        // One row a case, in `CompressionCase::ALL`'s order: disabled, then lz4.
        let expected = [
            [
                counts([8504, 8], 8509),
                compressed(counts([61, 1], 66), 8394, 0),
            ],
            // 10 packets of 9 messages, 7 + 9 · 130 = 1,177 bytes each, fill the window.
            [
                counts([11770, 10], 11775),
                compressed(counts([2740, 3], 2745), 13921, 0),
            ],
            // Random bytes do not compress: every one of the 10 packets goes uncompressed.
            [
                counts([11770, 10], 11775),
                compressed(counts([11770, 10], 11775), 0, 10),
            ],
            // One message a packet, 7 + 1,026 = 1,033 bytes, 11 of them in the window.
            [
                counts([11363, 11], 11368),
                compressed(counts([92, 1], 97), 16331, 0),
            ],
        ];
        for (case, row) in CompressionCase::ALL.into_iter().zip(expected) {
            for (&mode, counts) in CompressionMode::ALL.iter().zip(row) {
                assert_eq!(case.run(mode), counts, "{} {}", case.name, mode.name());
            }
        }
    }
}
