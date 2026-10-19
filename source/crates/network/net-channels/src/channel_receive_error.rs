#[derive(thiserror::Error, Debug)]
pub enum ChannelReceiveError {
    #[error("A message was received without a message ID")]
    MissingMessageId,
    #[error("fragmented message declared an invalid fragment count: {fragments}")]
    InvalidFragmentCount { fragments: u64 },
    #[error("fragment index {fragment_index} is outside fragment count {fragments}")]
    InvalidFragmentIndex { fragment_index: u64, fragments: u64 },
    #[error("fragment count changed while reassembling message: expected {expected}, got {actual}")]
    FragmentCountMismatch { expected: usize, actual: usize },
    #[error("{fragments} fragments can not fit a message of at most {max_message_len} bytes")]
    FragmentCountTooLarge {
        fragments: u64,
        max_message_len: usize,
    },
    #[error(
        "fragments describe a message of at least {min_len} bytes, above the channel's {max_message_len}"
    )]
    MessageTooLarge {
        min_len: usize,
        max_message_len: usize,
    },
    #[error(
        "fragment compression changed while reassembling message: expected {expected}, got {actual}"
    )]
    FragmentCompressionMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    #[error("fragmented message completed before receiving compression metadata")]
    MissingFragmentCompression,
    #[error("fragment uses unsupported compression: {compression}")]
    UnsupportedFragmentCompression { compression: &'static str },
    #[error("compressed fragment payload could not be decompressed")]
    FragmentDecompressionFailed,
    #[error("decompressed fragment payload size {actual} exceeds configured limit {limit}")]
    FragmentDecompressedPayloadTooLarge { actual: usize, limit: usize },
    #[error("non-final fragment is empty")]
    EmptyNonFinalFragment,
    #[error("non-final fragment has size {actual}, expected {expected}")]
    InvalidNonFinalFragmentLen { actual: usize, expected: usize },
    #[error("final fragment has size {actual}, maximum {max}")]
    InvalidFinalFragmentLen { actual: usize, max: usize },
}
