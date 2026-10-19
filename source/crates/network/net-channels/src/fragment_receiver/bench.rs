//! `atomic/transport/fragment_receiver/reassemble/…`: a message's fragments, received in order
//! by a fresh [`FragmentReceiver`] until it returns the message.

use bytes::Bytes;
use campfire_measure::Kind;
use campfire_measure::Scale;
use campfire_net_core::NetTick;
use criterion::{BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;

use crate::channel_setup::DEFAULT_MAX_MESSAGE_LEN;
use crate::compression_scratch::CompressionScratch;
use crate::compression_setup::CompressionSetup;
use crate::fragment_receiver::{
    FragmentCx, FragmentOutcome, FragmentReceiver, PartialExpiry, Reservation,
};
use crate::fragmented_message::FragmentedMessage;
use crate::message_id::MessageId;
use crate::packet::FRAGMENT_LEN;
use crate::receive_budget::ReceiveBudget;

const CX: FragmentCx = FragmentCx {
    expiry: PartialExpiry::Never,
    compression: CompressionSetup::DISABLED,
    max_message_len: DEFAULT_MAX_MESSAGE_LEN,
    reservation: Reservation::Limited,
};

/// Messages of 2, 16 and 128 fragments.
const FRAGMENTS: [usize; 3] = [2, 16, 128];

pub(crate) fn atomic(criterion: &mut Criterion) {
    let mut group = Kind::Atomic.group(criterion, "reassemble", Scale::Sample);
    for fragments in FRAGMENTS {
        let message: Bytes = (0..fragments * FRAGMENT_LEN)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>()
            .into();
        let split = FragmentedMessage::split(
            MessageId(0),
            message.clone(),
            FRAGMENT_LEN,
            CompressionSetup::DISABLED,
            &mut CompressionScratch::default(),
        );
        let built = split.fragment_body();
        assert_eq!(built.len(), fragments);
        group.bench_function(
            BenchmarkId::from_parameter(format!("{fragments}_fragments")),
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
