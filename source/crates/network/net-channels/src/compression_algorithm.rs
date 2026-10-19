/// Compression algorithm used for transport packet payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    #[cfg(feature = "compression_lz4")]
    Lz4,
}
