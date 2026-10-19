use crate::audit::Audit;
use campfire_net_bytes::Writer;
use std::collections::VecDeque;
use std::io::Write;

/// A sender whose messages wait for their acks keeps them alive: each frame writes one 64-byte
/// message and drops the one written 10 frames before. The writer reuses its chunks once their
/// messages are dropped, so a frame allocates nothing.
#[test]
fn a_writer_with_messages_alive() {
    let mut writer = Writer::default();
    let mut alive = VecDeque::with_capacity(11);
    let message = [7u8; 64];
    Audit::new().run(|| {
        writer.write_all(&message).unwrap();
        alive.push_back(writer.take_written());
        if alive.len() > 10 {
            alive.pop_front();
        }
    });
}
