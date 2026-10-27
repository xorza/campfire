use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An aircraft's slow death, which falls: a `JetSlowDeathBehavior` or a
/// `HelicopterSlowDeathBehavior`, as their module data read them: its share of gravity,
/// `FallHowFast`; a jet's `RollRate`, in radians a frame; a helicopter's spiral, its
/// `SpiralOrbitTurnRate` in degrees a second, its `SpiralOrbitForwardSpeed` in world units a
/// second, its `SpiralOrbitForwardSpeedDamping`, 1 by default, its `MaxBraking` in world units a
/// second squared, 99,999 by default, its `MinSelfSpin` and `MaxSelfSpin` in degrees a second,
/// its `SelfSpinUpdateAmount` in degrees and its `SelfSpinUpdateDelay` in milliseconds; what it
/// plays as it starts,
/// as it hits the ground, as it blows up after `blow_up_ms`, and a jet as it dies on the ground;
/// and what it plays that the import leaves, a jet's secondary and a helicopter's blade.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniFall {
    pub(crate) kind: FallKind,
    pub(crate) gravity: f32,
    pub(crate) roll: f32,
    pub(crate) turn: f32,
    pub(crate) speed: f32,
    pub(crate) damping: f32,
    pub(crate) braking: f32,
    pub(crate) spin: [f32; 2],
    pub(crate) spin_change: f32,
    pub(crate) spin_delay_ms: f32,
    pub(crate) start: FallMoment,
    pub(crate) hit: FallMoment,
    pub(crate) blow_up_ms: f32,
    pub(crate) blow_up: FallMoment,
    pub(crate) ground: FallMoment,
    pub(crate) left: FallMoment,
}

/// Which aircraft falls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallKind {
    Jet,
    Helicopter,
}

/// What a moment of a fall plays: an FX list and an object creation list, each by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FallMoment {
    pub(crate) effect: Option<String>,
    pub(crate) objects: Option<String>,
}

impl IniFall {
    /// The module a `Behavior` line of the class `class`, of either case, opens, as the game
    /// makes it, every field 0 and none; none for a class of no fall.
    pub(crate) fn module(class: &str) -> Option<Self> {
        let kind = if class.eq_ignore_ascii_case("jetslowdeathbehavior") {
            FallKind::Jet
        } else if class.eq_ignore_ascii_case("helicopterslowdeathbehavior") {
            FallKind::Helicopter
        } else {
            return None;
        };
        Some(Self {
            kind,
            gravity: 0.0,
            roll: 0.0,
            turn: 0.0,
            speed: 0.0,
            damping: 1.0,
            braking: 99_999.0,
            spin: [0.0; 2],
            spin_change: 0.0,
            spin_delay_ms: 0.0,
            start: FallMoment::default(),
            hit: FallMoment::default(),
            blow_up_ms: 0.0,
            blow_up: FallMoment::default(),
            ground: FallMoment::default(),
            left: FallMoment::default(),
        })
    }

    /// Takes the module's line of `word`, if it is one of the fall's fields; whether it was.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        let named = || -> Result<Option<String>, IniError> {
            let value = at.value()?;
            Ok((!value.eq_ignore_ascii_case("none")).then(|| value.to_owned()))
        };
        let jet = self.kind == FallKind::Jet;
        match word {
            "fallhowfast" => self.gravity = at.real()? / 100.0,
            "rollrate" if jet => self.roll = at.real()?,
            "spiralorbitturnrate" if !jet => self.turn = at.real()?,
            "spiralorbitforwardspeed" if !jet => self.speed = at.real()?,
            "spiralorbitforwardspeeddamping" if !jet => self.damping = at.real()?,
            "maxbraking" if !jet => self.braking = at.real()?,
            "selfspinupdateamount" if !jet => self.spin_change = at.real()?,
            "selfspinupdatedelay" if !jet => self.spin_delay_ms = at.real()?,
            "minselfspin" if !jet => self.spin[0] = at.real()?,
            "maxselfspin" if !jet => self.spin[1] = at.real()?,
            "fxinitialdeath" if jet => self.start.effect = named()?,
            "oclinitialdeath" if jet => self.start.objects = named()?,
            "fxhitground" => self.hit.effect = named()?,
            "oclhitground" => self.hit.objects = named()?,
            "fxfinalblowup" => self.blow_up.effect = named()?,
            "oclfinalblowup" => self.blow_up.objects = named()?,
            "delayfinalblowupfromhitground" if jet => self.blow_up_ms = at.real()?,
            "delayfromgroundtofinaldeath" if !jet => self.blow_up_ms = at.real()?,
            "fxongrounddeath" if jet => self.ground.effect = named()?,
            "oclongrounddeath" if jet => self.ground.objects = named()?,
            "fxsecondary" | "fxblade" => self.left.effect = named()?,
            "oclsecondary" | "oclblade" | "oclejectpilot" => self.left.objects = named()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}
