use crate::compression_algorithm::CompressionAlgorithm;

const DEFAULT_MIN_PAYLOAD_LEN: usize = 128;

const DEFAULT_MAX_DECOMPRESSED_PAYLOAD_LEN: usize = 64 * 1024;

/// How a link compresses each packed payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressionSetup {
    /// `None` disables compression.
    pub algorithm: Option<CompressionAlgorithm>,
    /// Do not attempt compression for payload bodies smaller than this value.
    pub min_payload_len: usize,
    /// Maximum decompressed packet payload body accepted on receive.
    pub max_decompressed_payload_len: usize,
}

impl CompressionSetup {
    pub const DISABLED: Self = Self {
        algorithm: None,
        min_payload_len: DEFAULT_MIN_PAYLOAD_LEN,
        max_decompressed_payload_len: DEFAULT_MAX_DECOMPRESSED_PAYLOAD_LEN,
    };

    #[cfg(feature = "compression_lz4")]
    pub const LZ4: Self = Self {
        algorithm: Some(CompressionAlgorithm::Lz4),
        min_payload_len: DEFAULT_MIN_PAYLOAD_LEN,
        max_decompressed_payload_len: DEFAULT_MAX_DECOMPRESSED_PAYLOAD_LEN,
    };

    pub const fn is_enabled(&self) -> bool {
        self.algorithm.is_some()
    }
}

impl Default for CompressionSetup {
    fn default() -> Self {
        Self::DISABLED
    }
}
