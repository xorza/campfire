use crate::compression_algorithm::CompressionAlgorithm;

const DEFAULT_MIN_PAYLOAD_SIZE: usize = 128;

const DEFAULT_MAX_DECOMPRESSED_PAYLOAD_SIZE: usize = 64 * 1024;

/// Configuration for post-pack packet payload compression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressionConfig {
    /// `None` disables compression.
    pub algorithm: Option<CompressionAlgorithm>,
    /// Do not attempt compression for payload bodies smaller than this value.
    pub min_payload_size: usize,
    /// Maximum decompressed packet payload body accepted on receive.
    pub max_decompressed_payload_size: usize,
}

impl CompressionConfig {
    pub const DISABLED: Self = Self {
        algorithm: None,
        min_payload_size: DEFAULT_MIN_PAYLOAD_SIZE,
        max_decompressed_payload_size: DEFAULT_MAX_DECOMPRESSED_PAYLOAD_SIZE,
    };

    #[cfg(feature = "compression_lz4")]
    pub const LZ4: Self = Self {
        algorithm: Some(CompressionAlgorithm::Lz4),
        min_payload_size: DEFAULT_MIN_PAYLOAD_SIZE,
        max_decompressed_payload_size: DEFAULT_MAX_DECOMPRESSED_PAYLOAD_SIZE,
    };

    pub const fn is_enabled(&self) -> bool {
        self.algorithm.is_some()
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self::DISABLED
    }
}
