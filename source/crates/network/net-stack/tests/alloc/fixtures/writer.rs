use std::collections::VecDeque;

use campfire_net_bytes::WriteInteger;
use campfire_net_bytes::Writer;

use crate::fixtures::AUDIT;

/// A sender whose messages wait for their acks keeps them alive: each frame writes one 64-byte
/// message and drops the one written 10 frames before. The writer reuses its chunks once their
/// messages are dropped, so a frame allocates nothing.
#[test]
fn a_writer_with_messages_alive() {
    let mut writer = Writer::default();
    let mut alive = VecDeque::with_capacity(11);
    let message = [7u8; 64];
    AUDIT.run(|| {
        writer.write_bytes(&message);
        alive.push_back(writer.take_written());
        if alive.len() > 10 {
            alive.pop_front();
        }
    });
}
