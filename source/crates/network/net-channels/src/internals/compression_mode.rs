use crate::compression_config::CompressionConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionMode {
    Disabled,
    #[cfg(feature = "compression_lz4")]
    Lz4,
}

impl CompressionMode {
    /// The modes this build has: LZ4 only with `compression_lz4`.
    pub const ALL: &[Self] = &[
        Self::Disabled,
        #[cfg(feature = "compression_lz4")]
        Self::Lz4,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            #[cfg(feature = "compression_lz4")]
            Self::Lz4 => "lz4",
        }
    }

    pub const fn config(self) -> CompressionConfig {
        match self {
            Self::Disabled => CompressionConfig::DISABLED,
            #[cfg(feature = "compression_lz4")]
            Self::Lz4 => CompressionConfig::LZ4,
        }
    }
}
