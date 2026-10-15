use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_actions::ActionBook;
use campfire_declare::ByType;
use campfire_math::{Num, Vec3};
use campfire_script::View;
use campfire_sim::{SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentOnce};
use campfire_units::DeliveryEntity;
use campfire_values::{ActionId, ActionStart, DamageKind, Rank, UnitType};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

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

/// How a projectile flies: homing on a unit, or along a line, a unit vector on the ground or in
/// space, for `range` meters, with the unit its action aimed at, if one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flight {
    Homing {
        target: StableId,
    },
    Line {
        direction: Vec3,
        range: Num,
        aimed: Option<StableId>,
    },
}

impl Flight {
    pub(crate) const fn homing(target: StableId) -> Flight {
        Flight::Homing { target }
    }

    /// A flight along `direction` for `range`, aimed at `aimed` if a unit.
    pub(crate) const fn line(direction: Vec3, range: Num, aimed: Option<StableId>) -> Flight {
        Flight::Line {
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
    Attack {
        /// The weapon's action, which its damage names.
        action: ActionId,
        rank: Rank,
        amount: Num,
        kind: DamageKind,
        roll: Num,
    },
    Action {
        action: ActionId,
        rank: Rank,
        start: Option<ActionStart>,
        group: StableId,
    },
}

impl Projectile {
    /// `None` unless a line's range is not negative, an attack's amount is not negative, and its
    /// roll is at least 0 and less than 1.
    pub fn new(source: StableId, flight: Flight, payload: Payload) -> Option<Projectile> {
        let flies = match flight {
            Flight::Homing { .. } => true,
            Flight::Line { range, .. } => range >= Num::ZERO,
        };
        let carries = match payload {
            Payload::Attack { amount, roll, .. } => {
                amount >= Num::ZERO && Num::ZERO <= roll && roll < Num::ONE
            }
            Payload::Action { .. } => true,
        };
        (flies && carries).then_some(Projectile {
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

    // A projectile of a type with no flight, of an action the book lacks or at a rank past its
    // ranks, or of a damage kind the mode lacks, has no rules for its flight or its hit; its
    // action's start lies within the world's bound, as every place does.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let specs = world.resource::<ByType<ProjectileSpec>>();
        let flies = world
            .get::<UnitType>(entity)
            .is_some_and(|&unit_type| specs.get(unit_type).is_some());
        let book = world.resource::<ActionBook>();
        let carries = match self.payload {
            Payload::Attack {
                action, rank, kind, ..
            } => {
                book.ranked(action, rank) && world.non_send::<View>().names().has_damage_kind(kind)
            }
            Payload::Action {
                action,
                rank,
                start,
                ..
            } => book.ranked(action, rank) && start.is_none_or(ActionStart::within_bound),
        };
        flies && carries
    }
}

impl Replication for Projectile {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}

/// A snapshot is untrusted, so a projectile that `new` refuses fails to decode.
impl<'de> Deserialize<'de> for Projectile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Projectile, D::Error> {
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
        Projectile::new(source, flight, payload)
            .ok_or_else(|| D::Error::custom("projectile out of its limits"))
    }
}
