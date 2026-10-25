use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{FlagSet, IniLine};

/// What a condition state plays, as `ModelConditionInfo` holds it: its clips, in order; whether
/// they are the default state's copy, which its first clip line replaces; the kinds of clip lines
/// it read, `GOT_IDLE_ANIMS` and `GOT_NONIDLE_ANIMS`, a line of `None` too; its mode; its flags;
/// the range of its speed; its `TransitionKey`; and the key of the state it lets finish, its
/// `WaitForStateToFinishIfPossible`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniAnimation {
    pub(crate) clips: Vec<IniClip>,
    copied: bool,
    lines: ClipLines,
    pub(crate) mode: IniMode,
    pub(crate) flags: AnimationFlags,
    pub(crate) speed: [f32; 2],
    pub(crate) key: Option<String>,
    pub(crate) waits_for: Option<String>,
}

/// A clip a state names, as `W3DAnimationInfo` holds it: its name, `<hierarchy>.<animation>` in
/// lowercase, the distance its unit walks in one run of it, 0 for none, and whether it is idle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniClip {
    pub(crate) name: String,
    pub(crate) distance: f32,
    pub(crate) idle: bool,
}

/// The kinds of clip lines a state read: of idle clips, and of others.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ClipLines {
    idle: bool,
    other: bool,
}

/// An `AnimationMode`, as `TheAnimModeNames` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IniMode {
    Manual,
    Loop,
    Once,
    PingPong,
    LoopBackwards,
    OnceBackwards,
}

/// A state's `Flags`, each bit at its place in `AnimationFlags::NAMES`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AnimationFlags(u32);

/// A flag of `AnimationFlags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnimationFlag {
    RandomStart = 0,
    StartFrameFirst = 1,
    StartFrameLast = 2,
    MaintainFrame = 5,
    RestartWhenComplete = 6,
    MaintainFrame2 = 7,
    MaintainFrame3 = 8,
    MaintainFrame4 = 9,
}

impl IniAnimation {
    /// A copy of `self`, the default state's, as a later state or a transition starts, whose
    /// first clip line replaces its clips.
    pub(crate) fn copied(&self) -> Self {
        Self {
            copied: true,
            ..self.clone()
        }
    }

    /// Reads `at`, a line of a state of the first word `word`, when it is a field of its
    /// animation, as `ModelConditionInfo`'s fields read it; whether it was one.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        let line = at.number;
        match word {
            "animation" | "idleanimation" => {
                let name = at.tokens.get(1).ok_or(IniError::NoValue { line })?;
                let distance = match at.tokens.get(2) {
                    Some(_) => at.real_at(2)?,
                    None => 0.0,
                };
                // `parseAnimation` repeats a clip as often as its line says, at least once.
                let repeat = match at.tokens.get(3) {
                    Some(_) => usize::try_from(at.int_at(3)?.max(1)).expect("at least 1"),
                    None => 1,
                };
                if self.copied {
                    self.clips.clear();
                    self.copied = false;
                    self.lines = ClipLines::default();
                }
                let idle = word == "idleanimation";
                if idle {
                    self.lines.idle = true;
                } else {
                    self.lines.other = true;
                }
                if !name.eq_ignore_ascii_case("none") {
                    let clip = IniClip {
                        name: name.to_ascii_lowercase(),
                        distance,
                        idle,
                    };
                    self.clips.extend(std::iter::repeat_n(clip, repeat));
                }
            }
            "animationmode" => {
                let mode = at.value()?;
                self.mode = IniMode::NAMES
                    .iter()
                    .find(|(name, _)| mode.eq_ignore_ascii_case(name))
                    .map(|&(_, mode)| mode)
                    .ok_or(IniError::NotListed { line })?;
            }
            "flags" => at.set_flags(&mut self.flags)?,
            "animationspeedfactorrange" => self.speed = [at.real_at(1)?, at.real_at(2)?],
            "transitionkey" => self.key = Some(at.value()?.to_ascii_lowercase()),
            "waitforstatetofinishifpossible" => {
                self.waits_for = Some(at.value()?.to_ascii_lowercase());
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Whether its clips are idle ones.
    pub(crate) fn idle(&self) -> bool {
        self.clips.iter().any(|clip| clip.idle)
    }

    /// Checks it as a condition state's at its block's `End` of the line `line`, as
    /// `parseConditionState` does: idle clip lines beside lines of others, or of a mode other than
    /// once, are refused.
    pub(crate) const fn check_state(&self, line: usize) -> Result<(), IniError> {
        let ClipLines { idle, other } = self.lines;
        if idle && (other || !self.mode.is_once()) {
            return Err(IniError::Animation { line });
        }
        Ok(())
    }

    /// Checks it as a transition's at its block's `End` of the line `line`: idle clip lines, a
    /// mode other than once, and a key of its own are refused.
    pub(crate) const fn check_transition(&self, line: usize) -> Result<(), IniError> {
        let keyed = self.key.is_some() || self.waits_for.is_some();
        if self.lines.idle || !self.mode.is_once() || keyed {
            return Err(IniError::Animation { line });
        }
        Ok(())
    }
}

impl Default for IniAnimation {
    /// A state's own before its fields, as `ModelConditionInfo::clear` leaves it: no clip, once,
    /// no flag, its own rate, and no key.
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            copied: false,
            lines: ClipLines::default(),
            mode: IniMode::Once,
            flags: AnimationFlags::default(),
            speed: [1.0, 1.0],
            key: None,
            waits_for: None,
        }
    }
}

impl IniMode {
    /// Each mode's name, of either case.
    const NAMES: [(&str, Self); 6] = [
        ("manual", Self::Manual),
        ("loop", Self::Loop),
        ("once", Self::Once),
        ("loop_pingpong", Self::PingPong),
        ("loop_backwards", Self::LoopBackwards),
        ("once_backwards", Self::OnceBackwards),
    ];

    /// Whether it plays once, forwards or backwards.
    pub(crate) const fn is_once(self) -> bool {
        matches!(self, Self::Once | Self::OnceBackwards)
    }
}

impl AnimationFlags {
    /// The flags' names, `ACBitsNames`, by their bits.
    const NAMES: [&str; 10] = [
        "randomstart",
        "start_frame_first",
        "start_frame_last",
        "adjust_height_by_construction_percent",
        "pristine_bone_pos_in_final_frame",
        "maintain_frame_across_states",
        "restart_anim_when_complete",
        "maintain_frame_across_states2",
        "maintain_frame_across_states3",
        "maintain_frame_across_states4",
    ];

    pub(crate) const fn holds(self, flag: AnimationFlag) -> bool {
        self.0 & (1 << flag as u32) != 0
    }
}

impl FlagSet for AnimationFlags {
    fn clear(&mut self) {
        self.0 = 0;
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        let Some(bit) = Self::NAMES
            .iter()
            .position(|known| name.eq_ignore_ascii_case(known))
        else {
            return false;
        };
        if on {
            self.0 |= 1 << bit;
        } else {
            self.0 &= !(1 << bit);
        }
        true
    }
}
