use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;
use crate::zero_hour::format::ini::object_ini::ini_fall::IniFall;

/// An object's `SlowDeathBehavior`, the jet's and the helicopter's that extend it, or
/// `FXListDie`, as its module data and its `DieMuxData` read it: the time its object stays dead
/// before it is destroyed, its `DestructionDelay` in milliseconds; its weight among the slow
/// deaths a death may take, its `ProbabilityModifier`; the effect list an `FXListDie` plays, its
/// `DeathFX`, empty for none, and none for a slow death; a slow death's phases, and its sink,
/// `SinkDelay` in milliseconds and `SinkRate` in world units a second; an aircraft's fall; and,
/// each as its last line says, whether its `DeathTypes` hold `NORMAL`, its `VeterancyLevels`
/// hold `REGULAR`, and its `RequiredStatus` names a status.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniDeath {
    pub(crate) delay_ms: f32,
    pub(crate) weight: i32,
    pub(crate) effect: Option<String>,
    pub(crate) phases: [DeathPhase; 3],
    pub(crate) sink_delay_ms: f32,
    pub(crate) sink_rate: f32,
    pub(crate) fall: Option<IniFall>,
    normal: bool,
    regular: bool,
    requires: bool,
}

/// What a slow death's phase, `INITIAL`, `MIDPOINT` or `FINAL` by its place, names, of which
/// `doPhaseStuff` plays one of each kind drawn at random: its FX lists and its object creation
/// lists, each in the order given, and how many weapons it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeathPhase {
    pub(crate) effects: Vec<String>,
    pub(crate) objects: Vec<String>,
    pub(crate) weapons: u32,
}

impl DeathPhase {
    /// A phase that names nothing.
    pub(crate) const NONE: Self = Self {
        effects: Vec::new(),
        objects: Vec::new(),
        weapons: 0,
    };
}

impl IniDeath {
    /// The deaths a `DeathTypes` line names, `TheDeathNames`.
    const DEATH_TYPES: [&str; 21] = [
        "normal",
        "none",
        "crushed",
        "burned",
        "exploded",
        "poisoned",
        "toppled",
        "flooded",
        "suicided",
        "lasered",
        "detonated",
        "splatted",
        "poisoned_beta",
        "extra_2",
        "extra_3",
        "extra_4",
        "extra_5",
        "extra_6",
        "extra_7",
        "extra_8",
        "poisoned_gamma",
    ];
    /// The levels a `VeterancyLevels` line names, `TheVeterancyNames`.
    const VETERANCY_LEVELS: [&str; 4] = ["regular", "veteran", "elite", "heroic"];

    /// The module a `Behavior` line of the class `class`, of either case, opens, as the game
    /// makes it: no delay, a weight of 10, no effect, no phase and no sink, every death, level and
    /// status; none for a class that is no slow death, an aircraft's or its own, nor an
    /// `FXListDie`.
    pub(crate) fn module(class: &str) -> Option<Self> {
        let fall = IniFall::module(class);
        let effect = if class.eq_ignore_ascii_case("slowdeathbehavior") || fall.is_some() {
            None
        } else if class.eq_ignore_ascii_case("fxlistdie") {
            Some(String::new())
        } else {
            return None;
        };
        Some(Self {
            delay_ms: 0.0,
            weight: 10,
            effect,
            phases: [DeathPhase::NONE, DeathPhase::NONE, DeathPhase::NONE],
            sink_delay_ms: 0.0,
            sink_rate: 0.0,
            fall,
            normal: true,
            regular: true,
            requires: false,
        })
    }

    /// Whether it applies to a normal death of an object of the first veterancy that holds no
    /// status, as `DieMuxData::isDieApplicable` tests it.
    pub(crate) const fn applies(&self) -> bool {
        self.normal && self.regular && !self.requires
    }

    /// Takes the module's line of `word`, as `SlowDeathBehaviorModuleData` and `DieMuxData`
    /// read it; a death type or a level of no name the game lists fails the read.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        if let Some(fall) = &mut self.fall
            && fall.field(word, at)?
        {
            return Ok(());
        }
        match word {
            "sinkdelay" => self.sink_delay_ms = at.real()?,
            "sinkrate" => self.sink_rate = at.real()?,
            "destructiondelay" => self.delay_ms = at.real()?,
            "probabilitymodifier" => self.weight = at.int()?,
            "deathfx" => {
                if let Some(effect) = &mut self.effect {
                    at.value()?.clone_into(effect);
                }
            }
            "fx" | "ocl" | "weapon" if self.effect.is_none() => {
                let phase = at.value()?;
                let phase = ["initial", "midpoint", "final"]
                    .iter()
                    .position(|name| phase.eq_ignore_ascii_case(name))
                    .ok_or(IniError::NotListed { line: at.number })?;
                let named = at.tokens.get(2).copied();
                let named = named.ok_or(IniError::NoValue { line: at.number })?;
                let phase = &mut self.phases[phase];
                match word {
                    "fx" => phase.effects.push(named.to_owned()),
                    "ocl" => phase.objects.push(named.to_owned()),
                    _ => phase.weapons += 1,
                }
            }
            "deathtypes" => self.normal = Self::holds(at, &Self::DEATH_TYPES, "normal")?,
            "veterancylevels" => {
                self.regular = Self::holds(at, &Self::VETERANCY_LEVELS, "regular")?;
            }
            "requiredstatus" => {
                let named = at.tokens[1..].iter();
                self.requires = named
                    .clone()
                    .any(|status| !status.eq_ignore_ascii_case("none"));
            }
            _ => {}
        }
        Ok(())
    }

    /// Whether the list of `at` holds `name` of `names`, as `INI::parseDeathTypeFlags` reads it:
    /// from all of them, `ALL` all, `NONE` none, `+` one more and `-` one less; any other token
    /// fails the read.
    fn holds(at: &IniLine<'_>, names: &[&str], name: &str) -> Result<bool, IniError> {
        let mut holds = true;
        for &token in &at.tokens[1..] {
            let changed = match token.split_at_checked(1) {
                _ if token.eq_ignore_ascii_case("all") => {
                    holds = true;
                    continue;
                }
                _ if token.eq_ignore_ascii_case("none") => {
                    holds = false;
                    continue;
                }
                Some(("+", changed)) => (changed, true),
                Some(("-", changed)) => (changed, false),
                _ => return Err(IniError::NotListed { line: at.number }),
            };
            let (changed, set) = changed;
            if !names
                .iter()
                .any(|known| changed.eq_ignore_ascii_case(known))
            {
                return Err(IniError::NotListed { line: at.number });
            }
            if changed.eq_ignore_ascii_case(name) {
                holds = set;
            }
        }
        Ok(holds)
    }
}
