use crate::compression_setup::CompressionSetup;

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

    pub const fn setup(self) -> CompressionSetup {
        match self {
            Self::Disabled => CompressionSetup::DISABLED,
            #[cfg(feature = "compression_lz4")]
            Self::Lz4 => CompressionSetup::LZ4,
        }
    }
}
