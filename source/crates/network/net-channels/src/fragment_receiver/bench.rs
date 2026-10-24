//! `atomic/fragment_receiver/reassemble_…`: a message's fragments, received in order by a fresh
//! [`FragmentReceiver`] until it returns the message, its throughput the fragments.

use bytes::Bytes;
use campfire_measure::{Kind, Scale};
use campfire_net_core::NetTick;
use criterion::{BatchSize, Criterion};
use std::hint::black_box;

use crate::channel_setup::DEFAULT_MAX_MESSAGE_LEN;
use crate::fragment_receiver::{FragmentCx, FragmentOutcome, FragmentReceiver, PartialExpiry};
use crate::fragmented_message::FragmentedMessage;
use crate::message_id::MessageId;
use crate::packet::FRAGMENT_LEN;
use crate::receive_budget::{ReceiveBudget, Reservation};

const CX: FragmentCx = FragmentCx {
    expiry: PartialExpiry::Never,
    max_message_len: DEFAULT_MAX_MESSAGE_LEN,
    reservation: Reservation::Limited,
};

/// Messages of 2, 16 and 128 fragments.
const FRAGMENTS: [usize; 3] = [2, 16, 128];

pub(crate) fn atomic(criterion: &mut Criterion) {
    let mut group = Kind::Atomic.group(criterion, "fragment_receiver", Scale::Sample);
    for fragments in FRAGMENTS {
        let message: Bytes = (0..fragments * FRAGMENT_LEN)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>()
            .into();
        let split = FragmentedMessage::split(MessageId(0), message.clone(), FRAGMENT_LEN);
        let built = split.fragment_body();
        assert_eq!(built.len(), fragments);
        group.throughput(campfire_measure::elements(fragments));
        group.bench_function(
            campfire_measure::case("reassemble", fragments, "fragment"),
            |bencher| {
                bencher.iter_batched(
                    || (FragmentReceiver::new(), built.clone()),
                    |(mut receiver, built)| {
                        let mut budget = ReceiveBudget::new(usize::MAX);
                        let mut reassembled = FragmentOutcome::Pending;
                        for fragment in built {
                            reassembled = receiver
                                .receive_fragment(fragment, NetTick(0), &CX, &mut budget)
                                .unwrap();
                        }
                        debug_assert!(matches!(
                            &reassembled,
                            FragmentOutcome::Complete { bytes, .. } if *bytes == message
                        ));
                        black_box(reassembled)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}
