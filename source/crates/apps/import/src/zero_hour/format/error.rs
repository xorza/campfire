use thiserror::Error;

/// Why a float of the game's files is not a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum FloatError {
    /// Infinite, or not a number.
    #[error("not finite")]
    NotFinite,
    /// Past `±2³⁹`, what a `Num` holds.
    #[error("outside the range of Num")]
    OutOfRange,
}

/// Bytes of the original that end inside a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("it ends inside a field")]
pub(crate) struct CutShort;

/// Why a WAV file of the original does not decode (09-zero-hour.md, A sound is PCM, the music the
/// game's MP3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum WavError {
    /// It does not start with a RIFF `WAVE` header.
    #[error("it is no RIFF WAVE file")]
    NotRiff,
    /// A chunk's header ends inside the file's end.
    #[error("a chunk ends inside its header")]
    CutShort,
    /// It holds no `fmt ` chunk of its fields.
    #[error("it holds no format")]
    NoFormat,
    /// It holds no `data` chunk.
    #[error("it holds no data")]
    NoData,
    /// Its encoding is neither PCM of 16 bits nor IMA ADPCM of 4.
    #[error("its encoding {tag} of {bits} bits is neither 16-bit PCM nor 4-bit IMA ADPCM")]
    Encoding { tag: u16, bits: u16 },
    /// It has no channel, or an ADPCM block too short for its headers or of a part of a run.
    #[error("its channels or its blocks do not hold")]
    Layout,
    /// Its samples a second, as 16-bit PCM of its channels, make more bytes a second than a
    /// `u32` counts.
    #[error("its rate makes more bytes a second than a WAV counts")]
    Rate,
}
