use std::collections::BTreeSet;

use campfire_client_files::{AttackPhase, ConditionTest, LifeShare, SiteStage};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_production::Production;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::UpgradeEffect;
use crate::zero_hour::format::ini::object_ini::{IniObject, ObjectIni};
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::upgrade_import;

/// What each condition flag of Zero Hour's draw modules tests of a unit, as its looks' conditions
/// name them (09-zero-hour.md, A condition is the game's name for a test): `NIGHT` and `SNOW` the
/// map's; `DAMAGED` and `REALLYDAMAGED` its life above and at most the shares of
/// `UnitDamagedThreshold` and `UnitReallyDamagedThreshold`, each apart from the other, as
/// `calcDamageState` sets one damage state; `MOVING` its walk; `DYING` and `RUBBLE` its death,
/// which a slow death keeps it for, as `RUBBLE` is the damage state of no life; `AWAITING_CONSTRUCTION` and
/// `PARTIALLY_CONSTRUCTED` a site before and after a builder first built it, and
/// `ACTIVELY_BEING_CONSTRUCTED` the engine's tag of a site that grew in the tick, as a dozer's
/// `DozerAIUpdate` sets them; `PREATTACK_<slot>`, `FIRING_<slot>`, `BETWEEN_FIRING_SHOTS_<slot>`
/// and `RELOADING_<slot>` each phase of an attack of the weapons of the slot `A`, `B` or `C`,
/// the look's group at that slot's place, and `USING_WEAPON_<slot>` any of them; `WEAPONSET_PLAYER_UPGRADE` the tag of a
/// `WeaponSetUpgrade`; a flag an upgrade module of an object sets the tag of its name, which the
/// module's modifier gives (09-zero-hour.md, An upgrade's look is a tag); and any other flag
/// none.
#[derive(Debug)]
pub(crate) struct LookFlags {
    damaged: LifeShare,
    really_damaged: LifeShare,
    weapon_upgrade: DeclaredName,
    upgraded: BTreeSet<DeclaredName>,
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

    /// The flags' tests of `game_data`'s thresholds, the tags of `names`, and the flags the
    /// upgrade modules of `objects` set; an error for thresholds that do not fall from 1 to above
    /// 0.
    pub(crate) fn of(
        game_data: &GameDataIni,
        names: &RulesNames,
        objects: &ObjectIni,
    ) -> Result<Self, ZeroHourError> {
        let share = |threshold: f32| {
            game_float::to_num(threshold).map_err(|_| ZeroHourError::DamageThresholds)
        };
        let damaged = share(game_data.damaged_threshold)?;
        let really_damaged = share(game_data.really_damaged_threshold)?;
        let shares =
            LifeShare::new(really_damaged, damaged).zip(LifeShare::new(Num::ZERO, really_damaged));
        let (damaged, really_damaged) = shares.ok_or(ZeroHourError::DamageThresholds)?;
        let modules = objects.objects().flat_map(IniObject::upgrades);
        let flags = modules.filter(|module| {
            matches!(
                module.effect,
                UpgradeEffect::PowerPlant | UpgradeEffect::ModelCondition { .. }
            )
        });
        let upgraded = flags
            .filter_map(|module| upgrade_import::look_tag(module).ok().flatten())
            .collect();
        Ok(Self {
            damaged,
            really_damaged,
            weapon_upgrade: names.weapon_upgrade.clone(),
            upgraded,
        })
    }

    /// The test of `flag` when it is an attack's phase of a weapon slot, or its use at any phase,
    /// as `WeaponSet::getModelConditionForWeaponSlot` sets `USING_WEAPON_<slot>` beside each.
    fn attack(flag: &str) -> Option<ConditionTest> {
        let phases = [
            ("preattack_", Some(AttackPhase::PreAttack)),
            ("firing_", Some(AttackPhase::Firing)),
            ("between_firing_shots_", Some(AttackPhase::Between)),
            ("reloading_", Some(AttackPhase::Reloading)),
            ("using_weapon_", None),
        ];
        phases.into_iter().find_map(|(prefix, phase)| {
            let group = match flag.strip_prefix(prefix)? {
                "a" => 0,
                "b" => 1,
                "c" => 2,
                _ => return None,
            };
            Some(ConditionTest::Attack { group, phase })
        })
    }

    /// The condition of `flag`, a flag of a state in lowercase, and its test; an error for a
    /// flag that is no declared name.
    pub(crate) fn condition(&self, flag: &str) -> Result<FlagCondition, ZeroHourError> {
        let name =
            DeclaredName::new(flag).ok_or_else(|| ZeroHourError::ConditionFlag(flag.to_owned()))?;
        let test = match flag {
            attack if let Some(test) = Self::attack(attack) => test,
            "night" | "snow" => ConditionTest::Map,
            "damaged" => ConditionTest::Life(self.damaged),
            "reallydamaged" => ConditionTest::Life(self.really_damaged),
            "dying" | "rubble" => ConditionTest::Dead,
            "moving" => ConditionTest::Moving {
                still_ms: Self::STILL_MS,
            },
            "awaiting_construction" => ConditionTest::Site {
                stage: SiteStage::Waiting,
            },
            "partially_constructed" => ConditionTest::Site {
                stage: SiteStage::Started,
            },
            "actively_being_constructed" => ConditionTest::Tag {
                tag: DeclaredName::new(Production::BEING_BUILT.name())
                    .expect("an engine tag's name is a name"),
            },
            "weaponset_player_upgrade" => ConditionTest::Tag {
                tag: self.weapon_upgrade.clone(),
            },
            _ if self.upgraded.contains(&name) => ConditionTest::Tag { tag: name.clone() },
            _ => ConditionTest::Never,
        };
        Ok(FlagCondition { name, test })
    }
}

#[cfg(test)]
mod tests;
