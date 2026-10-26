use std::num::NonZeroI64;

use campfire_actions::{SlotKind, SlotKindBook};
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::{Num, Rounding, SinCos, Vec3};
use campfire_script::UnknownName;
use campfire_sim::MatchRate;
use campfire_units::TurnStep;
use campfire_values::{Named, UnitType};

use crate::barrels::Barrel;
use crate::turret::turret_aim::TurretAim;
use crate::turret::turret_data::{TurretData, TurretPitchData};

pub(crate) mod aiming;
pub(crate) mod turret_aim;
pub(crate) mod turret_data;

/// A unit type's turret as a match runs it (04-capabilities/combat.md, A unit type may have a
/// turret): how far it turns a tick toward its target, and back toward its rest at half that,
/// how near its target's bearing and pitch it lets an attack start, the kind of slot whose
/// attacks it carries, the point of its unit's frame it turns about, and its pitch, if it
/// pitches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turret {
    pub(crate) turn: TurnStep,
    pub(crate) back: TurnStep,
    pub(crate) tolerance: Num,
    pub(crate) slot: SlotKind,
    pub(crate) pivot: Vec3,
    pub(crate) pitch: Option<Pitching>,
}

/// A turret's pitch as a match runs it (04-capabilities/combat.md, A turret may pitch): how far
/// it pitches a tick toward its aim, and back toward its rest at half that, the pitch it fires
/// at, if one, the least it aims at a target, its rest, each in degrees above the level, and the
/// point of its unit's frame it pitches about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pitching {
    pub(crate) turn: TurnStep,
    pub(crate) back: TurnStep,
    pub(crate) fire: Option<Num>,
    pub(crate) least: Num,
    pub(crate) rest: Num,
    pub(crate) pivot: Vec3,
}

impl Turret {
    /// The turret of `data`, its attacks those of the slot kind `slot`, its turns a tick at
    /// `rate`, each rounded once and at least a bit, so a turret always turns.
    pub fn of(data: &TurretData, slot: SlotKind, rate: MatchRate) -> Self {
        let [turn, back] = Self::steps(data.turn_rate, rate);
        Self {
            turn,
            back,
            tolerance: data.tolerance_deg,
            slot,
            pivot: data.pivot.get(),
            pitch: data.pitch.map(|pitch| Pitching::of(&pitch, rate)),
        }
    }

    /// The turns a tick of `turn_rate` degrees a second at `rate`, toward an aim and back at
    /// half of it, each rounded once and at least a bit.
    fn steps(turn_rate: Num, rate: MatchRate) -> [TurnStep; 2] {
        let hz = NonZeroI64::from(rate.hz()).get();
        [1, 2].map(|divisor| {
            let step = turn_rate
                .checked_div_int(hz * divisor, Rounding::NearestEven)
                .expect("a turn rate over a tick rate fits");
            TurnStep::new(step.max(Num::EPSILON)).expect("a turn rate is positive")
        })
    }
}

impl Turret {
    /// `barrel` as the turret at `aim` carries it in its unit's frame: pitched by the aim's pitch
    /// about the level axis across the unit through its pitch's pivot, then turned by the aim's
    /// angle about the vertical through its pivot, as `Weapon::calcProjectileLaunchPosition`
    /// moves a launch bone (04-capabilities/combat.md, A shot leaves its weapon's barrel).
    pub(crate) fn carried(&self, aim: TurretAim, barrel: Barrel) -> Barrel {
        let Barrel { mut at, mut way } = barrel;
        if let Some(pitch) = self.pitch {
            let raise = SinCos::of_degrees(aim.pitch());
            at = (at - pitch.pivot).rotated_z(raise) + pitch.pivot;
            way = way.rotated_z(raise);
        }
        let turn = SinCos::of_degrees(aim.angle());
        Barrel {
            at: (at - self.pivot).rotated_y(turn) + self.pivot,
            way: way.rotated_y(turn),
        }
    }
}

impl Pitching {
    fn of(data: &TurretPitchData, rate: MatchRate) -> Self {
        let [turn, back] = Turret::steps(data.turn_rate, rate);
        Self {
            turn,
            back,
            fire: data.fire_deg,
            least: data.least_deg,
            rest: data.rest_deg,
            pivot: data.pivot.get(),
        }
    }
}

impl PerId<UnitType> for Turret {
    const SCRIPTS_ONLY: bool = false;
}

impl Turret {
    /// The turret of each unit type that stands with one, its attacks those of its slot kind; an
    /// error for a kind the mode does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let kinds = &cx.book::<SlotKindBook>()?.0;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(turret) = unit.data.section::<TurretData>() else {
                continue;
            };
            let slot = kinds.names().named(turret.slot.as_str()).ok_or_else(|| {
                let at = BookPlace::UnitType(unit.place());
                let name = UnknownName::new(SlotKind::NAMES, &turret.slot);
                BookError::unknown(unit.package, at, name)
            })?;
            book.set(unit.unit_type, Self::of(turret, slot, cx.rate()));
        }
        Ok(book.build())
    }
}

#[cfg(test)]
mod tests;
