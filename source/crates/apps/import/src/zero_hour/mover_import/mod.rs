use std::collections::{BTreeMap, BTreeSet};

use campfire_common::DeclaredName;
use campfire_math::{Num, Vec3};
use campfire_navigation::{LocomotorAppearance, LocomotorData, ZeroHourLocomotor};
use campfire_stats::HurtData;
use campfire_values::{EngineStat, Number, Share, Stat};
use serde::Serialize;

use crate::zero_hour::error::TypeError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::locomotor_ini::{Appearance, Locomotor};
use crate::zero_hour::format::ini::object_ini::ini_object::IniObject;

/// A type's Zero Hour mover, of a locomotor of `TREADS`, `TWO_LEGS`, `FOUR_WHEELS` or `MOTORCYCLE`
/// that drives it on the ground (04-capabilities/navigation.md, Each appearance moves as its
/// mover): its `locomotor`, its acceleration in meters a second squared, and its hurt, of a
/// locomotor whose damaged numbers differ from its own (04-capabilities/stats.md, A hurt type may
/// hold a modifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImportedMover {
    pub(crate) locomotor: ZeroHourLocomotor,
    pub(crate) acceleration: Num,
    pub(crate) hurt: Option<MoverHurt>,
}

/// A mover's hurt: the share of its life at and below which it moves at its locomotor's damaged
/// numbers, the game's `UnitReallyDamagedThreshold`, and what its stats change by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MoverHurt {
    pub(crate) below: Share,
    pub(crate) change: HurtChange,
}

/// What a hurt mover's stats change by: its `SpeedDamaged` less its `Speed`, its
/// `TurnRateDamaged` less its `TurnRate` and its `AccelerationDamaged` less its `Acceleration`,
/// each the difference of the numbers its stats hold, so the hurt stat is the damaged number
/// exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HurtChange {
    pub(crate) speed: Num,
    pub(crate) turn_rate: Num,
    pub(crate) acceleration: Num,
}

/// The modifiers of the hurt movers, one for each change, shared among the types of that change,
/// `really_damaged_<n>` from 1 in the changes' order.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MoverImport {
    names: BTreeMap<HurtChange, DeclaredName>,
    pub(crate) modifiers: BTreeMap<DeclaredName, HurtModifier>,
}

/// A hurt's modifier as the rules data writes it: the amount it adds to each stat it changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HurtModifier {
    stats: BTreeMap<Stat, Number>,
}

impl ImportedMover {
    /// The mover of `object` that `locomotor` drives, of the game's `really_damaged` threshold;
    /// none for an appearance no mover drives as; an error for a number the engine's mover does
    /// not hold.
    pub(crate) fn of(
        object: &IniObject,
        locomotor: &Locomotor,
        really_damaged: f32,
    ) -> Result<Option<Self>, TypeError> {
        let appearance = match locomotor.appearance {
            Appearance::Treads => LocomotorAppearance::Treads,
            Appearance::Legs => LocomotorAppearance::TwoLegs,
            Appearance::Wheels => LocomotorAppearance::FourWheels,
            Appearance::Motorcycle => LocomotorAppearance::Motorcycle,
            Appearance::Other | Appearance::Thrust | Appearance::Hover | Appearance::Wings => {
                return Ok(None);
            }
        };
        let num = |value: f32| game_float::to_num(value).map_err(|_| TypeError::Locomotor);
        // A number past the world's bound, as the template's `BIGNUM` of a braking or a turning
        // speed it leaves out, is the bound.
        let bounded = |value: f32| num(value).map(|value| value.min(Vec3::BOUND));
        let geometry = object.geometry();
        let mover = ZeroHourLocomotor {
            appearance,
            braking: bounded(locomotor.braking())?,
            min_speed: num(locomotor.min_speed)?,
            min_turn_speed: bounded(locomotor.min_turn_speed())?,
            pivot: num(locomotor.pivot * geometry.bounding_circle())?,
            backwards: locomotor.backwards,
            three_point: num(5.0 * geometry.major)?,
            lateral_friction: num(object.lateral_friction())?,
            close_enough: num(locomotor.close_enough())?,
            corrects: !object.is_dozer(),
        };
        if !mover.holds() {
            return Err(TypeError::Locomotor);
        }
        let change = HurtChange {
            speed: num(locomotor.speed_damaged())? - num(locomotor.speed)?,
            turn_rate: num(locomotor.turn_rate_damaged())? - num(locomotor.turn_rate)?,
            acceleration: num(locomotor.acceleration_damaged())? - num(locomotor.acceleration)?,
        };
        let hurt = if change == HurtChange::NONE {
            None
        } else {
            let below = really_damaged.to_string().parse();
            let below = below.map_err(|_| TypeError::Locomotor)?;
            Some(MoverHurt { below, change })
        };
        Ok(Some(Self {
            locomotor: mover,
            acceleration: num(locomotor.acceleration)?,
            hurt,
        }))
    }

    /// Its `locomotor` section.
    pub(crate) const fn data(&self) -> LocomotorData {
        LocomotorData::ZeroHour(self.locomotor)
    }
}

impl HurtChange {
    /// No change.
    const NONE: Self = Self {
        speed: Num::ZERO,
        turn_rate: Num::ZERO,
        acceleration: Num::ZERO,
    };
}

impl MoverImport {
    /// The modifiers of the hurts of `movers`, the movers of the import's types.
    pub(crate) fn of<'m>(movers: impl IntoIterator<Item = &'m ImportedMover>) -> Self {
        let changes: BTreeSet<HurtChange> = movers
            .into_iter()
            .filter_map(|mover| Some(mover.hurt?.change))
            .collect();
        let mut import = Self::default();
        for (at, change) in (1..).zip(changes) {
            let name = DeclaredName::new(&format!("really_damaged_{at}"))
                .expect("a modifier's name the import gives");
            let stats = [
                (EngineStat::MoveSpeed, change.speed),
                (EngineStat::TurnRate, change.turn_rate),
                (EngineStat::Acceleration, change.acceleration),
            ]
            .into_iter()
            .filter(|&(_, by)| by != Num::ZERO)
            .map(|(stat, by)| (Stat::Engine(stat), Number::from(by)))
            .collect();
            import.names.insert(change, name.clone());
            import.modifiers.insert(name, HurtModifier { stats });
        }
        import
    }

    /// The `hurt` of `mover`, of the modifier of its change; none for a mover of no hurt.
    pub(crate) fn hurt(&self, mover: &ImportedMover) -> Option<HurtData> {
        let hurt = mover.hurt?;
        Some(HurtData {
            below: hurt.below,
            modifier: self.names[&hurt.change].clone(),
        })
    }
}

#[cfg(test)]
mod tests;
