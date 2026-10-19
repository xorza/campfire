//! The crate's criterion cases, which `benches/net-messages.rs` runs.
//!
//! `integration/send_receive/…`: frames that carry messages from the server to one client.

use campfire_measure::Kind;
use campfire_measure::Scale;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_transport::internals::Channel1;
use criterion::{BatchSize, BenchmarkId, Criterion};

use crate::fixtures::messages_stack::MessagesStack;
use crate::fixtures::protocol::StringMessage;
use crate::message_sender::MessageSender;

const MESSAGES: [usize; 5] = [0, 10, 100, 1000, 10000];

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    campfire_measure::without_logs(criterion, send_receive);
}

fn send_receive(criterion: &mut Criterion) {
    let mut group = Kind::Integration.group(criterion, "send_receive", Scale::Frame);
    for messages in MESSAGES {
        group.bench_function(
            BenchmarkId::from_parameter(campfire_measure::count(messages, "message")),
            |bencher| {
                bencher.iter_batched_ref(
                    || {
                        let stepper = Stepper::new(MessagesStack, StepperSetup::single());
                        let batch = vec![StringMessage("a".to_owned()); messages];
                        (stepper, batch)
                    },
                    |(stepper, batch)| {
                        let mut client_of = stepper.client_of_mut(0);
                        let mut sender =
                            client_of.get_mut::<MessageSender<StringMessage>>().unwrap();
                        for message in batch.drain(..) {
                            sender.send::<Channel1>(message);
                        }
                        stepper.frame_step(1);
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}
