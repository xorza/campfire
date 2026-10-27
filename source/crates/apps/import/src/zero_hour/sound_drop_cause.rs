use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops an audio event, or a part of how the game plays it (09-zero-hour.md,
/// Sound).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum SoundDropCause {
    /// An event a weapon, an effect or an object names that no audio file defines.
    NoEvent,
    /// A file an event names that no archive holds; an event of none is dropped whole.
    NoFile,
    /// A file that does not decode as a WAV of 16-bit PCM or 4-bit IMA ADPCM.
    Encoding,
    /// An event that loops, as an engine's hum or a continuous weapon's fire.
    Loop,
    /// An event of a sound that starts or ends it, its `Attack` and `Decay`.
    AttackDecay,
    /// An event of other sounds by the time of day.
    TimeOfDay,
    /// An event of a delay before it plays.
    Delay,
    /// An event of a low pass filter.
    LowPass,
    /// An event the game plays under the shroud, which the client does not hear.
    Shrouded,
    /// An event whose pitch's or range's numbers do not hold, played at its own pitch, or flat.
    Numbers,
}
