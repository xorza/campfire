use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::{MoveStep, UnitEntity};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// What a unit of a Zero Hour locomotor carries from tick to tick (driving.md, A mover keeps a
/// velocity): its velocity on the ground plane, `[x, z]` in meters a tick, as `PhysicsBehavior`
/// keeps it; whether its locomotor brakes, its `IS_BRAKING`, and whether it braked as its last
/// drive ended, the object's `OBJECT_STATUS_BRAKING`, which its stop leaves; the factor of its
/// braking, `m_brakingFactor`, from 0 to 5; whether it backs, `MOVING_BACKWARDS`, and turns in
/// three points, `DOING_THREE_POINT_TURN`; and the ticks it drove since it was last more than
/// the donut distance from its route's end or stood, which `m_donutTimer` counts as a frame.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZeroHourDrive {
    pub(crate) velocity: [Num; 2],
    pub(crate) braking: bool,
    pub(crate) braked: bool,
    pub(crate) factor: Num,
    pub(crate) backwards: bool,
    pub(crate) three_point: bool,
    pub(crate) donut: u32,
}

impl ZeroHourDrive {
    /// Its velocity on the ground plane, `[x, z]` in meters a tick.
    pub const fn velocity(&self) -> [Num; 2] {
        self.velocity
    }

    /// Its speed on the ground plane, in meters a tick: its velocity's length, rounded once.
    pub(crate) fn speed(&self) -> Num {
        let [x, z] = self.velocity;
        Vec3::new(x, Num::ZERO, z).length()
    }

    /// The braking factor a locomotor starts with and drives at far from its goal.
    pub(crate) const FACTOR: Num = Num::ONE;

    /// The most a braking factor grows to, `MAX_BRAKING_FACTOR`.
    pub(crate) const MAX_FACTOR: Num = Num::int(5);

    /// It stopped, as `locoUpdate_maintainCurrentPosition` stops a ground locomotor each frame it
    /// has no goal: its velocity gone, its locomotor not braking and its donut wait begun again.
    #[must_use]
    pub(crate) const fn stopped(self) -> Self {
        Self {
            velocity: [Num::ZERO; 2],
            braking: false,
            donut: 0,
            ..self
        }
    }
}

impl Default for ZeroHourDrive {
    fn default() -> Self {
        Self {
            velocity: [Num::ZERO; 2],
            braking: false,
            braked: false,
            factor: Self::FACTOR,
            backwards: false,
            three_point: false,
            donut: 0,
        }
    }
}

impl SimComponent for ZeroHourDrive {
    const NAME: &'static str = "navigation.zero_hour_drive";
    type Host = UnitEntity;

    // Of a unit that walks, its velocity within the world's bound and its factor from 0 to its
    // most; a count past its locomotor's wait braking reads alike.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let [x, z] = self.velocity;
        world.get::<MoveStep>(entity).is_some()
            && Vec3::new(x, Num::ZERO, z).within_bound()
            && (Num::ZERO..=Self::MAX_FACTOR).contains(&self.factor)
    }
}

impl Replication for ZeroHourDrive {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}
