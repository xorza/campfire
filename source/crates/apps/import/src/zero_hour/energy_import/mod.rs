use std::collections::BTreeSet;

use campfire_actions::ActionKind;
use campfire_common::DeclaredName;
use campfire_production::{EmptyPower, ModePowerPatch, PowerSpeed, PowerSpeedData};
use campfire_values::{FilterData, FilterTag, RelationSet, Share};
use serde::Serialize;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::game_data_ini::LowEnergy;
use crate::zero_hour::import_names::NamedType;
use crate::zero_hour::rules_names::RulesNames;

/// Zero Hour's power (power.md, Power is production's): the mode's `[power]` of the speed of a
/// short player's production, of `GameData.ini`'s low-energy fields, over the rules package's
/// stat and shortage, as `ThingTemplate::calcTimeToBuild` slows a unit's and a structure's time
/// and not an upgrade's, and a player of no power and no use has a share of none
/// (`Energy::getEnergySupplyRatio`); the modifier a short player holds, which gives its powered
/// units the tag `disabled`, as `Player::onPowerBrownOutChange` disables each object of
/// `KindOf = POWERED` (power.md, A disabled unit is the rules package's), the mode's own as a
/// rules package holds no modifier; and the types whose objects give power, which its bots build
/// while short.
#[derive(Debug, Default)]
pub(crate) struct EnergyImport {
    pub(crate) power: ModePowerPatch,
    pub(crate) shortage: Option<ShortModifier>,
    pub(crate) plants: BTreeSet<DeclaredName>,
}

/// The modifier of a short player as the mode's data writes it, `[modifiers.<name>]` of its
/// `name`: the units it holds on, and the tag it gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ShortModifier {
    #[serde(skip)]
    pub(crate) name: DeclaredName,
    affects: FilterData,
    tags: Vec<DeclaredName>,
}

impl EnergyImport {
    /// The original's speed of a short player whose rate is none: 0.01.
    const FLOOR: Share = Share::billionths(10_000_000).expect("0.01 is a share");

    /// The power of `low`'s speed, of `names`' tags, and of `types`; an error for a speed the
    /// engine holds none of.
    pub(crate) fn of<'n, 'r: 'n>(
        low: LowEnergy,
        names: &RulesNames,
        types: impl Iterator<Item = NamedType<'n, 'r>>,
    ) -> Result<Self, ZeroHourError> {
        let share = |value: f32| game_float::to_share(value).ok_or(ZeroHourError::LowEnergy);
        // A speed of none is the original's floor of 0.01 (`calcTimeToBuild`), and a `min` past
        // the `max` is the `max`, which `min` then gives every short player, as the original's.
        let floor = |share: Share| if share.is_zero() { Self::FLOOR } else { share };
        let max = floor(share(low.max)?);
        let min = floor(share(low.min)?).min(max);
        let speed = PowerSpeedData {
            penalty: share(low.penalty)?,
            min,
            max,
            empty: EmptyPower::None,
            slows: vec![ActionKind::Train, ActionKind::Build],
        };
        PowerSpeed::new(&speed).map_err(|_| ZeroHourError::LowEnergy)?;
        let plants = types
            .filter(|named| named.object.energy() > 0)
            .map(|named| named.name.declared().clone())
            .collect();
        let powered = FilterTag {
            name: names.powered.clone(),
            negated: false,
        };
        let shortage = ShortModifier {
            name: names.underpowered.clone(),
            affects: FilterData {
                relations: RelationSet::Allies,
                tags: vec![powered],
            },
            tags: vec![names.disabled.clone()],
        };
        Ok(Self {
            power: ModePowerPatch {
                stat: None,
                short: Some(names.underpowered.clone()),
                speed: Some(speed),
            },
            shortage: Some(shortage),
            plants,
        })
    }
}

#[cfg(test)]
mod tests;
