use campfire_math::Num;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// How a look's state plays its animation (animated-looks.md, A state plays its animation by its
/// mode): its clips, of which it draws one each time it starts one, a clip as often as it is
/// named; whether they are idle clips, of which it draws another, not the last, as one completes;
/// its mode; the frame it starts at; the groups of states across which it keeps its frame, by
/// their bits; whether it starts again as it completes; and the range it draws its speed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateAnimation {
    clips: Vec<StateClip>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    idle: bool,
    mode: AnimationMode,
    #[serde(skip_serializing_if = "StartFrame::is_natural")]
    start: StartFrame,
    #[serde(skip_serializing_if = "is_zero")]
    maintain: u8,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    restart: bool,
    #[serde(skip_serializing_if = "SpeedRange::is_one")]
    speed: SpeedRange,
}

/// An animation a state plays, its glTF, and the distance in meters its unit walks in one run of
/// it, none for one played at its own rate whatever its unit's speed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateClip {
    pub animation: PackagePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stride: Option<Num>,
}

/// How a state's clip runs: held at its start frame; looped; once; to its end and back, again
/// and again; looped backwards; or once backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationMode {
    Manual,
    Loop,
    Once,
    PingPong,
    LoopBackwards,
    OnceBackwards,
}

/// The frame a clip starts at: its mode's own, its first for a forward mode and its last for a
/// backward one; one drawn at random; its first; or its last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartFrame {
    #[default]
    Natural,
    Random,
    First,
    Last,
}

/// The range a clip's speed is drawn from, as a factor of its own rate: above 0, its least at
/// most its most. Written as `[least, most]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeedRange {
    least: Num,
    most: Num,
}

impl StateAnimation {
    /// The groups of states a state may keep its frame across, each a bit.
    pub const MAINTAIN_GROUPS: u32 = 4;

    /// The animation of `clips`, played by `mode`; none for no clip, or for groups past the
    /// `MAINTAIN_GROUPS`.
    pub fn new(clips: Vec<StateClip>, mode: AnimationMode, play: StatePlay) -> Option<Self> {
        let StatePlay {
            idle,
            start,
            maintain,
            restart,
            speed,
        } = play;
        let holds = !clips.is_empty() && u32::from(maintain) >> Self::MAINTAIN_GROUPS == 0;
        holds.then_some(Self {
            clips,
            idle,
            mode,
            start,
            maintain,
            restart,
            speed,
        })
    }

    pub fn clips(&self) -> &[StateClip] {
        &self.clips
    }

    pub const fn idle(&self) -> bool {
        self.idle
    }

    pub const fn mode(&self) -> AnimationMode {
        self.mode
    }

    pub const fn start(&self) -> StartFrame {
        self.start
    }

    pub const fn maintain(&self) -> u8 {
        self.maintain
    }

    pub const fn restart(&self) -> bool {
        self.restart
    }

    pub const fn speed(&self) -> SpeedRange {
        self.speed
    }
}

/// How a state plays its clips, past its clips and its mode: whether they are idle clips, the
/// frame each starts at, the groups across which it keeps its frame, whether it starts again as
/// it completes, and the range of its speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatePlay {
    pub idle: bool,
    pub start: StartFrame,
    pub maintain: u8,
    pub restart: bool,
    pub speed: SpeedRange,
}

impl StartFrame {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    const fn is_natural(&self) -> bool {
        matches!(self, Self::Natural)
    }
}

impl SpeedRange {
    /// Its own rate, a factor of 1.
    pub const ONE: Self = Self {
        least: Num::ONE,
        most: Num::ONE,
    };

    /// The range from `least` to `most`; none unless `0 < least ≤ most`.
    pub fn new(least: Num, most: Num) -> Option<Self> {
        (Num::ZERO < least && least <= most).then_some(Self { least, most })
    }

    pub const fn least(self) -> Num {
        self.least
    }

    pub const fn most(self) -> Num {
        self.most
    }

    fn is_one(&self) -> bool {
        *self == Self::ONE
    }
}

impl Default for StatePlay {
    /// Clips that are not idle, of their mode's start, kept across no states, not started again,
    /// at their own rate.
    fn default() -> Self {
        Self {
            idle: false,
            start: StartFrame::Natural,
            maintain: 0,
            restart: false,
            speed: SpeedRange::ONE,
        }
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
const fn is_zero(groups: &u8) -> bool {
    *groups == 0
}

/// A package's file is untrusted, so an animation of no clip or of groups past four, and a
/// stride that is not above 0, fail to read.
impl<'de> Deserialize<'de> for StateAnimation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            clips: Vec<StateClip>,
            #[serde(default)]
            idle: bool,
            mode: AnimationMode,
            #[serde(default)]
            start: StartFrame,
            #[serde(default)]
            maintain: u8,
            #[serde(default)]
            restart: bool,
            #[serde(default = "one")]
            speed: SpeedRange,
        }
        const fn one() -> SpeedRange {
            SpeedRange::ONE
        }
        let Fields {
            clips,
            idle,
            mode,
            start,
            maintain,
            restart,
            speed,
        } = Fields::deserialize(deserializer)?;
        if clips
            .iter()
            .any(|clip| clip.stride.is_some_and(|stride| stride <= Num::ZERO))
        {
            return Err(D::Error::custom("a clip's stride is not above 0"));
        }
        let play = StatePlay {
            idle,
            start,
            maintain,
            restart,
            speed,
        };
        Self::new(clips, mode, play)
            .ok_or_else(|| D::Error::custom("an animation has no clip, or groups past four"))
    }
}

impl Serialize for SpeedRange {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        [self.least, self.most].serialize(serializer)
    }
}

/// A package's file is untrusted, so a range not above 0 or out of order fails to read.
impl<'de> Deserialize<'de> for SpeedRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [least, most] = <[Num; 2]>::deserialize(deserializer)?;
        Self::new(least, most)
            .ok_or_else(|| D::Error::custom("a speed range is not above 0 and in order"))
    }
}
