use campfire_client_files::{ConditionTest, LifeShare};
use campfire_common::DeclaredName;
use campfire_math::Num;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
use crate::zero_hour::rules_names::RulesNames;

/// What each condition flag of Zero Hour's draw modules tests of a unit, as its looks' conditions
/// name them (animated-looks.md, A condition is the game's name for a test): `NIGHT` and `SNOW` the
/// map's; `DAMAGED` and `REALLYDAMAGED` its life above and at most the shares of
/// `UnitDamagedThreshold` and `UnitReallyDamagedThreshold`, each apart from the other, as
/// `calcDamageState` sets one damage state; `MOVING` its walk; `WEAPONSET_PLAYER_UPGRADE` the tag
/// of a `WeaponSetUpgrade`; and any other flag none.
#[derive(Debug)]
pub(crate) struct LookFlags {
    damaged: LifeShare,
    really_damaged: LifeShare,
    weapon_upgrade: DeclaredName,
}

/// A flag's condition: its name, and its test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlagCondition {
    pub(crate) name: DeclaredName,
    pub(crate) test: ConditionTest,
}

impl LookFlags {
    /// `MOVING` clears once its object is held up for more than a quarter of a second,
    /// `getNumFramesBlocked() > LOGICFRAMES_PER_SECOND / 4` (`AIInternalMoveToState::update`).
    const STILL_MS: u32 = 250;

    /// The flags' tests of `game_data`'s thresholds and the tags of `names`; an error for
    /// thresholds that do not fall from 1 to above 0.
    pub(crate) fn of(game_data: &GameDataIni, names: &RulesNames) -> Result<Self, ZeroHourError> {
        let share = |threshold: f32| {
            game_float::to_num(threshold).map_err(|_| ZeroHourError::DamageThresholds)
        };
        let damaged = share(game_data.damaged_threshold)?;
        let really_damaged = share(game_data.really_damaged_threshold)?;
        let shares =
            LifeShare::new(really_damaged, damaged).zip(LifeShare::new(Num::ZERO, really_damaged));
        let (damaged, really_damaged) = shares.ok_or(ZeroHourError::DamageThresholds)?;
        Ok(Self {
            damaged,
            really_damaged,
            weapon_upgrade: names.weapon_upgrade.clone(),
        })
    }

    /// The condition of `flag`, a flag of a state in lowercase, and its test; an error for a
    /// flag that is no declared name.
    pub(crate) fn condition(&self, flag: &str) -> Result<FlagCondition, ZeroHourError> {
        let name =
            DeclaredName::new(flag).ok_or_else(|| ZeroHourError::ConditionFlag(flag.to_owned()))?;
        let test = match flag {
            "night" | "snow" => ConditionTest::Map,
            "damaged" => ConditionTest::Life(self.damaged),
            "reallydamaged" => ConditionTest::Life(self.really_damaged),
            "moving" => ConditionTest::Moving {
                still_ms: Self::STILL_MS,
            },
            "weaponset_player_upgrade" => ConditionTest::Tag {
                tag: self.weapon_upgrade.clone(),
            },
            _ => ConditionTest::Never,
        };
        Ok(FlagCondition { name, test })
    }
}

#[cfg(test)]
mod tests;
