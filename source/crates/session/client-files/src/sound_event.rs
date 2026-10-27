use std::num::NonZeroU16;

use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

/// A sound an effect plays and a look's voice names (02-engine-core.md, An event is the game's, as
/// data): its files, one for each play, as `pick` draws it; its volume, from 0 to 1, and the share
/// of it a play may lose, a play's volume drawn between `volume × (1 − loss)` and `volume`; its
/// pitch's range, a play's rate drawn between its two, 1 the file's own; its limit of plays at
/// once, none for no limit; its range, none for a flat sound; and who hears it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundEvent {
    pub files: Vec<PackagePath>,
    #[serde(default)]
    pub pick: SoundPick,
    pub volume: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub loss: f32,
    #[serde(default = "SoundEvent::own_pitch")]
    pub pitch: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<SoundLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<SoundRange>,
    pub heard_by: Hearers,
}

/// How a play draws its file: at random, never the last one again of a list of more than two, as
/// `AudioEventRTS::generateFilename` draws it; or each in turn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundPick {
    #[default]
    Random,
    InTurn,
}

/// The most plays of an event at once, past which a new play is dropped, or, when it
/// `interrupt`s, stops the oldest, as `MilesAudioManager::doesViolateLimit` counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundLimit {
    pub count: NonZeroU16,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupt: bool,
}

/// A positional sound's range: the distance in meters up to which it plays at its full volume, and
/// the distance at which it falls silent (02-engine-core.md, A sound falls off as the release
/// game's and fades to its range's end).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundRange {
    pub full: f32,
    pub silent: f32,
}

/// Who hears a sound, by the relation of its source's owner to the player: the owner alone, its
/// allies but the owner, its enemies, or everyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hearers {
    Owner,
    Allies,
    Enemies,
    Everyone,
}

impl SoundEvent {
    /// Whether it holds: a file at least, a volume and a loss of 0 to 1, a pitch's range of rates
    /// above 0 in order, and a range from no distance below 0 to one past it, each finite.
    pub fn holds(&self) -> bool {
        let share = |share: f32| (0.0..=1.0).contains(&share);
        let [low, high] = self.pitch;
        !self.files.is_empty()
            && share(self.volume)
            && share(self.loss)
            && low > 0.0
            && low <= high
            && high.is_finite()
            && self.range.is_none_or(SoundRange::holds)
    }

    const fn own_pitch() -> [f32; 2] {
        [1.0, 1.0]
    }
}

impl SoundRange {
    /// Whether its full range is of no distance below 0 and its silence past it, each finite.
    pub fn holds(self) -> bool {
        self.full >= 0.0 && self.silent > self.full && self.silent.is_finite()
    }
}

/// Whether `share` is 0, which the file leaves out.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's `skip_serializing_if` passes a reference"
)]
fn is_zero(share: &f32) -> bool {
    *share == 0.0
}
