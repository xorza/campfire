use crate::compression_scratch::CompressionScratch;
use crate::fragment_count::FragmentCount;
use crate::fragment_index::FragmentIndex;
use crate::fragmented_message::FragmentedMessage;
use crate::packet::FRAGMENT_LEN;
use campfire_net_core::Ticked;

use super::*;
use crate::channel_setup::DEFAULT_MAX_MESSAGE_LEN;

/// The fragments of `bytes` at `fragment_len`, compressed with `compression` when that helps.
fn split(
    id: MessageId,
    bytes: Bytes,
    fragment_len: usize,
    compression: CompressionSetup,
) -> Vec<FragmentBody> {
    FragmentedMessage::split(
        id,
        bytes,
        fragment_len,
        compression,
        &mut CompressionScratch::default(),
    )
    .fragment_body()
}

fn cx(compression: CompressionSetup) -> FragmentCx {
    FragmentCx {
        expiry: PartialExpiry::Never,
        compression,
        max_message_len: DEFAULT_MAX_MESSAGE_LEN,
        reservation: Reservation::Limited,
    }
}

/// Receives with room to spare, returning the completed message, if any.
fn receive(
    receiver: &mut FragmentReceiver,
    fragment: FragmentBody,
    tick: NetTick,
    cx: &FragmentCx,
) -> Result<Option<Ticked<Bytes>>> {
    let outcome =
        receiver.receive_fragment(fragment, tick, cx, &mut ReceiveBudget::new(usize::MAX))?;
    Ok(match outcome {
        FragmentOutcome::Complete { tick, bytes } => Some(Ticked::new(tick, bytes)),
        FragmentOutcome::Pending => None,
        FragmentOutcome::OverBudget => unreachable!("the budget is unbounded"),
    })
}

#[test]
fn fragments_out_of_order_rebuild_the_message() -> Result<()> {
    let mut receiver = FragmentReceiver::new();
    let message_len = FRAGMENT_LEN * 3 / 2;
    let message_bytes = Bytes::from(vec![1u8; message_len]);
    let fragments = split(
        MessageId(0),
        message_bytes.clone(),
        FRAGMENT_LEN,
        CompressionSetup::DISABLED,
    );

    assert_eq!(
        receive(
            &mut receiver,
            fragments[1].clone(),
            NetTick(1),
            &cx(CompressionSetup::DISABLED)
        )?,
        None
    );
    assert_eq!(
        receive(
            &mut receiver,
            fragments[0].clone(),
            NetTick(0),
            &cx(CompressionSetup::DISABLED)
        )?,
        Some(Ticked::new(NetTick(1), message_bytes))
    );
    Ok(())
}

/// The receiver is never told the sender's fragment size, which here is smaller than, equal to
/// and larger than the default. Every message is `2 * fragment_len + 26` bytes, so it splits
/// into fragments of `fragment_len`, `fragment_len` and 26 bytes, received in every order.
#[test]
fn fragments_of_any_sender_fragment_len_reassemble_in_any_order() -> Result<()> {
    const ORDERS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for fragment_len in [37, FRAGMENT_LEN, 2 * FRAGMENT_LEN] {
        // Every byte differs from its neighbours, so a fragment written at a wrong offset
        // changes the message.
        let message_bytes: Bytes = (0..2 * fragment_len + 26)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>()
            .into();
        let fragments = split(
            MessageId(4),
            message_bytes.clone(),
            fragment_len,
            CompressionSetup::DISABLED,
        );
        assert_eq!(
            fragments
                .iter()
                .map(|fragment| fragment.bytes.len())
                .collect::<Vec<_>>(),
            [fragment_len, fragment_len, 26]
        );

        for order in ORDERS {
            let mut receiver = FragmentReceiver::new();
            let mut result = None;
            for index in order {
                assert_eq!(result, None, "size {fragment_len}, order {order:?}");
                result = receive(
                    &mut receiver,
                    fragments[index].clone(),
                    NetTick(index as u32),
                    &cx(CompressionSetup::DISABLED),
                )?;
            }
            assert_eq!(
                result,
                Some(Ticked::new(NetTick(order[0] as u32), message_bytes.clone())),
                "size {fragment_len}, order {order:?}"
            );
        }
    }
    Ok(())
}

/// Feeds hand-made fragments of a message with `count` fragments, given as
/// `(fragment index, byte count)`, and returns the result of the last one.
fn receive_fragments(count: u64, fragments: &[(u64, usize)]) -> Result<Option<Ticked<Bytes>>> {
    receive_bounded_fragments(count, DEFAULT_MAX_MESSAGE_LEN, fragments)
}

fn receive_bounded_fragments(
    count: u64,
    max_message_len: usize,
    fragments: &[(u64, usize)],
) -> Result<Option<Ticked<Bytes>>> {
    let cx = FragmentCx {
        max_message_len,
        ..cx(CompressionSetup::DISABLED)
    };
    let mut receiver = FragmentReceiver::new();
    let mut result = Ok(None);
    for &(index, len) in fragments {
        assert!(matches!(result, Ok(None)), "{fragments:?}: {result:?}");
        let fragment = FragmentBody {
            message_id: MessageId(9),
            fragment_index: FragmentIndex(index),
            fragments: FragmentCount(count),
            compression: (index == 0).then_some(FragmentCompression::None),
            bytes: Bytes::from(vec![index as u8; len]),
        };
        result = receive(&mut receiver, fragment, NetTick(0), &cx);
    }
    result
}

/// With a 100-byte limit: 101 fragments can not fit before any size is known, nor can 2^62 - 1
/// (the count that aborted the process); three fragments of 50 bytes describe at least
/// 2 * 50 + 1 = 101 bytes, and a final fragment of 99 bytes at least 2 + 99 = 101; three of 49
/// bytes fit.
#[test]
fn fragments_of_a_message_above_the_limit_are_refused() {
    assert!(matches!(
        receive_bounded_fragments(101, 100, &[(0, 1)]),
        Err(ChannelReceiveError::FragmentCountTooLarge {
            fragments: 101,
            max_message_len: 100
        })
    ));
    assert!(matches!(
        receive_bounded_fragments((1 << 62) - 1, 100, &[(5, 1)]),
        Err(ChannelReceiveError::FragmentCountTooLarge { .. })
    ));
    assert!(matches!(
        receive_bounded_fragments(3, 100, &[(0, 50)]),
        Err(ChannelReceiveError::MessageTooLarge {
            min_len: 101,
            max_message_len: 100
        })
    ));
    assert!(matches!(
        receive_bounded_fragments(3, 100, &[(2, 99)]),
        Err(ChannelReceiveError::MessageTooLarge {
            min_len: 101,
            max_message_len: 100
        })
    ));
    // No sender splits a message into more than 16,383 fragments, whatever the size limit.
    assert!(matches!(
        receive_bounded_fragments(16_384, usize::MAX, &[(0, 1)]),
        Err(ChannelReceiveError::FragmentCountTooLarge {
            fragments: 16_384,
            ..
        })
    ));
    let message = receive_bounded_fragments(3, 100, &[(0, 49), (2, 1), (1, 49)]).unwrap();
    assert_eq!(message.unwrap().value.len(), 99);
}

/// 130 one-byte fragments span three words of receipts; repeats of 63, 64 and 128 change
/// nothing, and the final fragment completes the message with every byte at its index.
#[test]
fn the_receipts_of_a_message_span_several_words() {
    let mut fragments = (0..129).map(|index| (index, 1)).collect::<Vec<_>>();
    fragments.extend([(63, 1), (64, 1), (128, 1), (129, 1)]);
    let message = receive_fragments(130, &fragments).unwrap().unwrap().value;
    assert_eq!(message, (0..130_u8).collect::<Vec<_>>());
}

#[test]
fn fragment_sizes_must_match_the_first_non_final_fragment() {
    assert!(matches!(
        receive_fragments(3, &[(0, 37), (1, 36)]),
        Err(ChannelReceiveError::InvalidNonFinalFragmentLen {
            actual: 36,
            expected: 37
        })
    ));
    assert!(matches!(
        receive_fragments(3, &[(1, 37), (0, 38)]),
        Err(ChannelReceiveError::InvalidNonFinalFragmentLen {
            actual: 38,
            expected: 37
        })
    ));
    assert!(matches!(
        receive_fragments(3, &[(0, 37), (2, 38)]),
        Err(ChannelReceiveError::InvalidFinalFragmentLen {
            actual: 38,
            max: 37
        })
    ));
    // The final fragment arrives before the size is known, so the first non-final fragment
    // rejects it.
    assert!(matches!(
        receive_fragments(3, &[(2, 38), (0, 37)]),
        Err(ChannelReceiveError::InvalidFinalFragmentLen {
            actual: 38,
            max: 37
        })
    ));
    assert!(matches!(
        receive_fragments(3, &[(0, 0)]),
        Err(ChannelReceiveError::EmptyNonFinalFragment)
    ));

    // A final fragment equal to the fragment size, and a message of one fragment with no
    // non-final fragment to set the size: 37 + 37 + 37 = 111 bytes, and 5 bytes.
    let complete = |result: Result<Option<Ticked<Bytes>>>| result.unwrap().unwrap().value.len();
    assert_eq!(
        complete(receive_fragments(3, &[(2, 37), (1, 37), (0, 37)])),
        111
    );
    assert_eq!(complete(receive_fragments(1, &[(0, 5)])), 5);
}

#[cfg(feature = "compression_lz4")]
#[test]
fn compressed_fragments_decompress_once_their_message_is_whole() -> Result<()> {
    let compression = CompressionSetup {
        min_payload_len: 0,
        max_decompressed_payload_len: FRAGMENT_LEN * 4,
        ..CompressionSetup::LZ4
    };
    let mut receiver = FragmentReceiver::new();
    let message_bytes = Bytes::from(vec![7u8; FRAGMENT_LEN * 3]);
    let fragments = split(
        MessageId(1),
        message_bytes.clone(),
        FRAGMENT_LEN,
        compression,
    );

    assert!(fragments.len() < 3);
    assert_eq!(fragments[0].compression, Some(FragmentCompression::Lz4));
    assert!(
        fragments
            .iter()
            .skip(1)
            .all(|fragment| fragment.compression.is_none())
    );

    let mut result = None;
    for (index, fragment) in fragments.into_iter().enumerate() {
        result = receive(
            &mut receiver,
            fragment,
            NetTick(index as u32),
            &cx(compression),
        )?;
    }

    assert_eq!(result, Some(Ticked::new(NetTick(0), message_bytes)));
    Ok(())
}

/// Each malformed fragment is refused with its own error: a count of 0, an index at the count, a
/// count that changes within a message, a first fragment without its compression, and a
/// compression that changes within a message.
#[test]
fn malformed_fragments_are_refused_with_their_error() {
    type Case<'a> = (&'a [FragmentBody], fn(&ChannelReceiveError) -> bool);
    let fragment = |index, count, compression| FragmentBody {
        message_id: MessageId(0),
        fragment_index: FragmentIndex(index),
        fragments: FragmentCount(count),
        compression,
        bytes: Bytes::from_static(&[7; 4]),
    };
    let none = Some(FragmentCompression::None);
    let cases: [Case<'_>; 5] = [
        (&[fragment(0, 0, none)], |error| {
            matches!(
                error,
                ChannelReceiveError::InvalidFragmentCount { fragments: 0 }
            )
        }),
        (&[fragment(2, 2, None)], |error| {
            matches!(
                error,
                ChannelReceiveError::InvalidFragmentIndex {
                    fragment_index: 2,
                    fragments: 2
                }
            )
        }),
        (&[fragment(1, 3, None), fragment(0, 2, none)], |error| {
            matches!(
                error,
                ChannelReceiveError::FragmentCountMismatch {
                    expected: 3,
                    actual: 2
                }
            )
        }),
        (&[fragment(0, 2, None)], |error| {
            matches!(error, ChannelReceiveError::MissingFragmentCompression)
        }),
        (
            &[
                fragment(0, 3, none),
                fragment(1, 3, Some(FragmentCompression::Lz4)),
            ],
            |error| {
                matches!(
                    error,
                    ChannelReceiveError::FragmentCompressionMismatch {
                        expected: "none",
                        actual: "lz4"
                    }
                )
            },
        ),
    ];
    for (fragments, expected) in cases {
        let mut receiver = FragmentReceiver::new();
        let (last, first) = fragments.split_last().unwrap();
        for fragment in first {
            let pending = receive(
                &mut receiver,
                fragment.clone(),
                NetTick(0),
                &cx(CompressionSetup::DISABLED),
            );
            assert!(matches!(pending, Ok(None)), "{pending:?}");
        }
        let error = receive(
            &mut receiver,
            last.clone(),
            NetTick(0),
            &cx(CompressionSetup::DISABLED),
        )
        .unwrap_err();
        assert!(expected(&error), "{error:?}");
    }
}
