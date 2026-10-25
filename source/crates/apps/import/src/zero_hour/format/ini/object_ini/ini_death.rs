use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An object's `SlowDeathBehavior`, as its module data and its `DieMuxData` read it: the time its
/// object stays dead before it is destroyed, its `DestructionDelay` in milliseconds; its weight
/// among the slow deaths a death may take, its `ProbabilityModifier`; and, each as its last line
/// says, whether its `DeathTypes` hold `NORMAL`, its `VeterancyLevels` hold `REGULAR`, and its
/// `RequiredStatus` names a status.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniDeath {
    pub(crate) delay_ms: f32,
    pub(crate) weight: i32,
    normal: bool,
    regular: bool,
    requires: bool,
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
    /// makes it: no delay, a weight of 10, every death, level and status; none for a class
    /// that is no `SlowDeathBehavior`.
    pub(crate) fn module(class: &str) -> Option<Self> {
        class
            .eq_ignore_ascii_case("slowdeathbehavior")
            .then_some(Self {
                delay_ms: 0.0,
                weight: 10,
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
        match word {
            "destructiondelay" => self.delay_ms = at.real()?,
            "probabilitymodifier" => self.weight = at.int()?,
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
