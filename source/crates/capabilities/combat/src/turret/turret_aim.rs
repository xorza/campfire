use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::Res;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_math::{Num, Vec3};
use campfire_sim::{SimComponent, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{Replication, SentPredicted};
use campfire_units::{Dead, Facing, TurnStep, UnitEntity};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::turret::Turret;

/// Where a unit's turret points: its angle against its body's facing, in degrees from `x`
/// towards `−z`, as a facing turns, taken modulo 360, and its pitch, in degrees above the level,
/// from −90 to 90 (04-capabilities/combat.md, A unit type may have a turret;
/// 04-capabilities/combat.md, A turret may pitch). Combat writes it in place each tick; its
/// owner's client predicts it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurretAim {
    angle: Facing,
    pitch: Num,
}

impl TurretAim {
    /// `angle` degrees against the body, taken modulo 360, and `pitch` degrees above the level.
    pub const fn of(angle: Num, pitch: Num) -> Self {
        Self {
            angle: Facing::of(angle),
            pitch,
        }
    }

    /// The rest of `turret`: along the body, at its pitch's rest, or level for a turret that
    /// does not pitch.
    pub(crate) fn rest(turret: &Turret) -> Self {
        let pitch = turret.pitch.map_or(Num::ZERO, |pitch| pitch.rest);
        Self::of(Num::ZERO, pitch)
    }

    /// Its angle's degrees, at least 0 and below 360.
    pub const fn angle(self) -> Num {
        self.angle.degrees()
    }

    /// Its pitch's degrees.
    pub const fn pitch(self) -> Num {
        self.pitch
    }

    /// The aim of `turret`, on a unit facing `facing` whose top stands at `top`, at a target
    /// whose place stands at `at`, both points in space: the bearing of `at` on the ground plane
    /// against the facing; and its pitch's fire pitch, or else the pitch of the way from `top` to
    /// `at`, no lower than its least, as `TurretAIAimTurretState::update` aims, level for a
    /// turret that does not pitch. None when the two stand on one point of the ground plane.
    pub(crate) fn toward(turret: &Turret, facing: Facing, top: Vec3, at: Vec3) -> Option<Self> {
        let way = Facing::along([at.x - top.x, at.z - top.z])?;
        let pitch = turret.pitch.map_or(Num::ZERO, |pitch| {
            pitch.fire.unwrap_or_else(|| {
                let ground = top.ground_offset(at).length();
                (at.y - top.y).atan2_degrees(ground).max(pitch.least)
            })
        });
        Some(Self::of(way.degrees() - facing.degrees(), pitch))
    }

    /// It turned toward `aim` by at most `turret`'s turns a tick, its angle the shorter way
    /// round.
    #[must_use]
    pub(crate) fn aimed(self, aim: Self, turret: &Turret) -> Self {
        let pitch = turret.pitch.map(|pitch| pitch.turn);
        self.turned(aim, turret.turn, pitch)
    }

    /// It turned back toward `turret`'s rest by at most half its turns a tick.
    #[must_use]
    pub(crate) fn rested(self, turret: &Turret) -> Self {
        let pitch = turret.pitch.map(|pitch| pitch.back);
        self.turned(Self::rest(turret), turret.back, pitch)
    }

    /// Whether its angle and its pitch each lie within `tolerance` degrees of `aim`'s, either
    /// way.
    pub(crate) const fn within(self, aim: Self, tolerance: Num) -> bool {
        let pitch = (self.pitch.to_bits() - aim.pitch.to_bits()).unsigned_abs();
        self.angle.apart(aim.angle).to_bits() <= tolerance.to_bits()
            && pitch <= tolerance.to_bits().unsigned_abs()
    }

    /// It turned toward `aim`, its angle by at most `step`, its pitch by at most `pitch`, and
    /// not at all with none.
    fn turned(self, aim: Self, step: TurnStep, pitch: Option<TurnStep>) -> Self {
        let left = aim.pitch - self.pitch;
        let pitched = pitch.map_or(Num::ZERO, |pitch| left.clamp(-pitch.get(), pitch.get()));
        Self {
            angle: self.angle.toward(aim.angle, Some(step)).facing,
            pitch: self.pitch + pitched,
        }
    }
}

/// A turret not at its rest is due at once, and stays held until it rests, as the aim turns it
/// each tick, toward its target or back.
impl TimedProcess for TurretAim {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (&'static Self, &'static UnitType, Has<Dead>);
    type Books = Res<'static, ById<UnitType, Turret>>;

    /// A turret off its rest turns each tick, but a dead unit's, which holds still until it
    /// lives again.
    fn due(
        turrets: &Res<'_, ById<UnitType, Turret>>,
        (aim, unit_type, dead): (&Self, &UnitType, bool),
    ) -> Option<Tick> {
        let turret = turrets.get(*unit_type)?;
        (!dead && *aim != Self::rest(turret)).then_some(Tick::ZERO)
    }
}

impl SimComponent for TurretAim {
    const NAME: &'static str = "combat.turret_aim";
    type Host = UnitEntity;

    // Its facing's own check holds its angle within a turn, its pitch lies from −90 to 90
    // degrees, and only a unit of a type with a turret has one.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let turrets = world.resource::<ById<UnitType, Turret>>();
        let turreted = world
            .get::<UnitType>(entity)
            .is_some_and(|&unit_type| turrets.get(unit_type).is_some());
        let level = Num::int(-90)..=Num::int(90);
        turreted && self.angle.check(world, entity) && level.contains(&self.pitch)
    }
}

impl Replication for TurretAim {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
