use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// `TheWeaponBonusNames`, of the game's build, which leaves `ALLOW_DEMORALIZE` out: a bonus's
/// condition, the two continuous-fire stages at 2 and 3.
pub(crate) const BONUS_CONDITIONS: [&str; 27] = [
    "GARRISONED",
    "HORDE",
    "CONTINUOUS_FIRE_MEAN",
    "CONTINUOUS_FIRE_FAST",
    "NATIONALISM",
    "PLAYER_UPGRADE",
    "DRONE_SPOTTING",
    "DEMORALIZED_OBSOLETE",
    "ENTHUSIASTIC",
    "VETERAN",
    "ELITE",
    "HERO",
    "BATTLEPLAN_BOMBARDMENT",
    "BATTLEPLAN_HOLDTHELINE",
    "BATTLEPLAN_SEARCHANDDESTROY",
    "SUBLIMINAL",
    "SOLO_HUMAN_EASY",
    "SOLO_HUMAN_NORMAL",
    "SOLO_HUMAN_HARD",
    "SOLO_AI_EASY",
    "SOLO_AI_NORMAL",
    "SOLO_AI_HARD",
    "TARGET_FAERIE_FIRE",
    "FANATICISM",
    "FRENZY_ONE",
    "FRENZY_TWO",
    "FRENZY_THREE",
];

/// The place of `CONTINUOUS_FIRE_MEAN` among `BONUS_CONDITIONS`, `FAST` the next.
pub(crate) const CONTINUOUS_FIRE_MEAN: usize = 2;

/// The place of `GARRISONED` among `BONUS_CONDITIONS`.
pub(crate) const GARRISONED: usize = 0;

/// The places of `ENTHUSIASTIC` and `SUBLIMINAL` among `BONUS_CONDITIONS`, a propaganda tower's.
pub(crate) const ENTHUSIASTIC: usize = 8;
pub(crate) const SUBLIMINAL: usize = 15;

/// The place of `PLAYER_UPGRADE` among `BONUS_CONDITIONS`.
pub(crate) const PLAYER_UPGRADE: usize = 5;

/// `TheWeaponBonusFieldNames`: what a bonus changes, the rate of fire at 3.
pub(crate) const BONUS_FIELDS: [&str; 5] =
    ["DAMAGE", "RADIUS", "RANGE", "RATE_OF_FIRE", "PRE_ATTACK"];

/// The place of `RATE_OF_FIRE` among `BONUS_FIELDS`.
pub(crate) const RATE_OF_FIRE: usize = 3;

/// The place of `DAMAGE` among `BONUS_FIELDS`.
pub(crate) const DAMAGE: usize = 0;

/// The place of `RANGE` among `BONUS_FIELDS`.
pub(crate) const RANGE: usize = 2;

/// The place of `VETERAN` among `BONUS_CONDITIONS`, `ELITE` and `HERO` the next two.
pub(crate) const VETERAN: usize = 9;

/// A `WeaponBonus` line, of a weapon or of `GameData.ini`'s set: its condition and its field, by
/// their places among `BONUS_CONDITIONS` and `BONUS_FIELDS`, and its value, its percent over
/// 100, as `INI::scanPercentToReal` divides it in the game's `f32`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WeaponBonusLine {
    pub(crate) condition: usize,
    pub(crate) field: usize,
    pub(crate) value: f32,
}

impl WeaponBonusLine {
    /// The bonus of the line `at`; an error for a condition or a field the game does not name.
    pub(crate) fn read(at: &IniLine<'_>) -> Result<Self, IniError> {
        let listed = |token: Option<&&str>, names: &[&str]| {
            let token = token.ok_or(IniError::NoValue { line: at.number })?;
            let found = names
                .iter()
                .position(|name| name.eq_ignore_ascii_case(token));
            found.ok_or(IniError::NotListed { line: at.number })
        };
        Ok(Self {
            condition: listed(at.tokens.get(1), &BONUS_CONDITIONS)?,
            field: listed(at.tokens.get(2), &BONUS_FIELDS)?,
            value: at.real_at(3)? / 100.0,
        })
    }
}
