use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::ByType;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::{Facing, TurnStep, UnitEntity};
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

use crate::turret::Turret;

/// The angle a unit's turret lies at against its body's facing, in degrees from `x` towards
/// `−z`, as a facing turns, taken modulo 360 (20-zero-hour-projectiles.md, A unit type may have
/// a turret). Combat writes it in place each tick; its owner's client predicts it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TurretAngle(Facing);

impl TurretAngle {
    /// Its rest, along the body.
    pub const REST: Self = Self(Facing::of(Num::ZERO));

    /// `angle` degrees against the body, taken modulo 360.
    pub const fn of(angle: Num) -> Self {
        Self(Facing::of(angle))
    }

    /// Its degrees, at least 0 and below 360.
    pub const fn degrees(self) -> Num {
        self.0.degrees()
    }

    /// The angle at which the turret of a unit at `from` facing `facing` points at `to`, on the
    /// ground plane; none when the two stand on one point.
    pub(crate) fn toward(facing: Facing, from: Vec3, to: Vec3) -> Option<Self> {
        let way = Facing::along([to.x - from.x, to.z - from.z])?;
        Some(Self::of(way.degrees() - facing.degrees()))
    }

    /// It turned toward `aim` the shorter way round, by at most `step`.
    #[must_use]
    pub(crate) fn turned(self, aim: Self, step: TurnStep) -> Self {
        Self(self.0.toward(aim.0, Some(step)).facing)
    }

    /// Whether it lies within `tolerance` degrees of `aim`, either way.
    pub(crate) const fn within(self, aim: Self, tolerance: Num) -> bool {
        self.0.apart(aim.0).to_bits() <= tolerance.to_bits()
    }
}

impl SimComponent for TurretAngle {
    const NAME: &'static str = "combat.turret_angle";
    type Host = UnitEntity;

    // Its facing's own check holds its angle within a turn; only a unit of a type with a turret
    // has one.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let turrets = world.resource::<ByType<Turret>>();
        let turreted = world
            .get::<UnitType>(entity)
            .is_some_and(|&unit_type| turrets.get(unit_type).is_some());
        turreted && self.0.check(world, entity)
    }
}

impl Replication for TurretAngle {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
