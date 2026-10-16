use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ProjectileFlight, ShotDamage};
use campfire_declare::ByType;
use campfire_math::{Num, Vec3};
use campfire_script::View;
use campfire_sim::{Position, SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentOnce};
use campfire_units::{DeliveryEntity, MapBook};
use campfire_values::{ActionId, ActionStart, Rank, UnitType};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::curve::{Curve, MAX_POINTS};
use crate::projectile_spec::ProjectileSpec;

/// A projectile unit in flight: whose it is, how it flies, at its type's speed, and what it
/// carries. None of it changes as it flies; its `FlightState` does.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
#[component(immutable)]
#[require(DeliveryEntity)]
pub struct Projectile {
    source: StableId,
    flight: Flight,
    payload: Payload,
}

/// How a projectile flies: homing on a unit; along a line from `from`, where it launched, a
/// unit vector on the ground or in space, for `range` meters, with the unit its action aimed at,
/// if one; or on a curve from `from` to `to`, points in space, in `points` points, with the unit
/// it aimed at, if one, which its end follows. Each step of a line ends at its place along the
/// line from `from`, and each point of a curve is the curve's value, rounded once, so no rounding
/// gathers over its steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flight {
    Homing {
        target: StableId,
    },
    Line {
        from: Vec3,
        direction: Vec3,
        range: Num,
        aimed: Option<StableId>,
    },
    Curve {
        from: Vec3,
        to: Vec3,
        aimed: Option<StableId>,
        points: u32,
    },
}

/// Where a curve launches: its first and its last control point, points in space, and the unit
/// it aims at, if one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CurveAim {
    pub(crate) from: Vec3,
    pub(crate) to: Vec3,
    pub(crate) aimed: Option<StableId>,
}

impl Flight {
    pub(crate) const fn homing(target: StableId) -> Self {
        Self::Homing { target }
    }

    /// The curve of `spec`, whose type flies one, as `aim` launches it over the ground of `map`:
    /// its count of points at the type's speed, ⌈length ÷ speed⌉.
    pub(crate) fn curve(aim: CurveAim, spec: &ProjectileSpec, map: &MapBook) -> Self {
        let CurveAim { from, to, aimed } = aim;
        let shape = spec.curve.expect("a curve's type has a curve");
        let curve = Curve::new(from, to, &shape, map.highest_along(from, to));
        Self::Curve {
            from,
            to,
            aimed,
            points: curve.count(spec.speed),
        }
    }

    /// How it flies, as its type does.
    pub(crate) const fn kind(self) -> ProjectileFlight {
        match self {
            Self::Homing { .. } => ProjectileFlight::Homing,
            Self::Line { .. } => ProjectileFlight::Line,
            Self::Curve { .. } => ProjectileFlight::Curve,
        }
    }

    /// A flight from `from` along `direction` for `range`, aimed at `aimed` if a unit.
    pub(crate) const fn line(
        from: Vec3,
        direction: Vec3,
        range: Num,
        aimed: Option<StableId>,
    ) -> Self {
        Self::Line {
            from,
            direction,
            range,
            aimed,
        }
    }
}

/// What a projectile carries: an attack's damage of `kind`, the rank of its weapon's slot and the
/// roll it drew, or the action at `rank` whose `on_hit` and `on_end` it runs, with the projectiles
/// of its cast in `group`, named by the first of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Payload {
    Attack(ShotDamage),
    Action {
        action: ActionId,
        rank: Rank,
        start: Option<ActionStart>,
        group: StableId,
    },
}

impl Projectile {
    /// `None` unless a line's range is not negative, a curve has from 1 to `MAX_POINTS` points,
    /// an attack's amount is not negative, and its roll is at least 0 and less than 1.
    pub fn new(source: StableId, flight: Flight, payload: Payload) -> Option<Self> {
        let flies = match flight {
            Flight::Homing { .. } => true,
            Flight::Line { range, .. } => range >= Num::ZERO,
            Flight::Curve { points, .. } => (1..=MAX_POINTS).contains(&points),
        };
        let carries = match payload {
            Payload::Attack(ShotDamage { amount, roll, .. }) => {
                amount >= Num::ZERO && Num::ZERO <= roll && roll < Num::ONE
            }
            Payload::Action { .. } => true,
        };
        (flies && carries).then_some(Self {
            source,
            flight,
            payload,
        })
    }

    pub(crate) const fn source(&self) -> StableId {
        self.source
    }

    pub const fn flight(&self) -> Flight {
        self.flight
    }

    pub(crate) const fn payload(&self) -> Payload {
        self.payload
    }

    /// Whether it homes on `id`.
    pub(crate) const fn homes_on(&self, id: StableId) -> bool {
        matches!(self.flight, Flight::Homing { target } if target.get() == id.get())
    }
}

impl SimComponent for Projectile {
    const NAME: &'static str = "projectiles.projectile";
    type Host = DeliveryEntity;

    // A projectile of a type with no flight or another flight, of an action the book lacks or at
    // a rank past its ranks, or of a damage kind the mode lacks, has no rules for its flight or
    // its hit; its action's start lies within the world's bound, as every place does.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let specs = world.resource::<ByType<ProjectileSpec>>();
        let flies = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| specs.get(unit_type))
            .is_some_and(|spec| spec.flight() == self.flight.kind());
        let book = world.resource::<ActionBook>();
        // A line's origin and a curve's ends lie within the world's bound, as every place does.
        let within = |at: Vec3| Position::from(at).within_bound();
        let origin = match self.flight {
            Flight::Line { from, .. } => within(from),
            Flight::Curve { from, to, .. } => within(from) && within(to),
            Flight::Homing { .. } => true,
        };
        let carries = match self.payload {
            Payload::Attack(ShotDamage {
                action, rank, kind, ..
            }) => {
                book.ranked(action, rank) && world.non_send::<View>().names().has_damage_kind(kind)
            }
            Payload::Action {
                action,
                rank,
                start,
                ..
            } => book.ranked(action, rank) && start.is_none_or(ActionStart::check),
        };
        flies && origin && carries
    }
}

impl Replication for Projectile {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}

/// A snapshot is untrusted, so a projectile that `new` refuses fails to decode.
impl<'de> Deserialize<'de> for Projectile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            source: StableId,
            flight: Flight,
            payload: Payload,
        }
        let Fields {
            source,
            flight,
            payload,
        } = Fields::deserialize(deserializer)?;
        Self::new(source, flight, payload)
            .ok_or_else(|| D::Error::custom("projectile out of its limits"))
    }
}
