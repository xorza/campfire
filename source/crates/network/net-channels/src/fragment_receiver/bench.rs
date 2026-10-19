//! `atomic/transport/fragment_receiver/reassemble/…`: a message's fragments, received in order
//! by a fresh [`FragmentReceiver`] until it returns the message.

use bytes::Bytes;
use campfire_measure::Kind;
use campfire_measure::Scale;
use campfire_net_core::NetTick;
use criterion::{BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;

use crate::channel_config::DEFAULT_MAX_MESSAGE_SIZE;
use crate::compression_config::CompressionConfig;
use crate::compression_scratch::CompressionScratch;
use crate::fragment_receiver::{
    FragmentContext, FragmentOutcome, FragmentReceiver, PartialExpiry, Reservation,
};
use crate::fragmented_message::FragmentedMessage;
use crate::message_id::MessageId;
use crate::packet::FRAGMENT_SIZE;
use crate::receive_budget::ReceiveBudget;

const CONTEXT: FragmentContext = FragmentContext {
    expiry: PartialExpiry::Never,
    compression: CompressionConfig::DISABLED,
    max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
    reservation: Reservation::Limited,
};

/// Messages of 2, 16 and 128 fragments.
const FRAGMENTS: [usize; 3] = [2, 16, 128];

pub(crate) fn atomic(criterion: &mut Criterion) {
    let mut group = Kind::Atomic.group(criterion, "reassemble", Scale::Sample);
    for fragments in FRAGMENTS {
        let message: Bytes = (0..fragments * FRAGMENT_SIZE)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>()
            .into();
        let split = FragmentedMessage::split(
            MessageId(0),
            message.clone(),
            FRAGMENT_SIZE,
            CompressionConfig::DISABLED,
            &mut CompressionScratch::default(),
        );
        let built = split.fragment_data();
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
                                .receive_fragment(fragment, NetTick(0), &CONTEXT, &mut budget)
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
