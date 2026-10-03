use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionBook;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::units::action_id::ActionId;
use crate::units::by_type::ByType;
use crate::units::script_view::View;
use crate::units::unit_type::UnitType;
use crate::values::action_start::ActionStart;
use crate::values::damage_kind::DamageKind;

/// A projectile unit in flight: whose it is, how it flies, at its type's speed, and what it
/// carries.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Projectile {
    source: StableId,
    flight: Flight,
    payload: Payload,
}

/// How a projectile flies: homing on a unit, or along a line, a unit vector on the ground or in
/// space, for `range` meters, with the unit its action aimed at, if one; `flown` meters of it so
/// far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Flight {
    Homing {
        target: StableId,
        flown: Num,
    },
    Line {
        direction: Vec3,
        flown: Num,
        range: Num,
        aimed: Option<StableId>,
    },
}

/// What a projectile carries: an attack's damage of `kind`, the rank of its weapon's slot and the
/// roll it drew, or the action at `rank` whose `on_hit` and `on_end` it runs, with the projectiles
/// of its cast in `group`, named by the first of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Payload {
    Attack {
        /// The weapon's action, which its damage names.
        action: ActionId,
        rank: u8,
        amount: Num,
        kind: DamageKind,
        roll: Num,
    },
    Action {
        action: ActionId,
        rank: u8,
        start: Option<ActionStart>,
        group: StableId,
    },
}

impl Projectile {
    /// `None` unless a line's distance flown is within its range, an attack's amount is not
    /// negative, and its roll is at least 0 and less than 1.
    pub(crate) fn new(source: StableId, flight: Flight, payload: Payload) -> Option<Projectile> {
        let flies = match flight {
            Flight::Homing { flown, .. } => flown >= Num::ZERO,
            Flight::Line { flown, range, .. } => Num::ZERO <= flown && flown <= range,
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

    pub(crate) const fn flight(&self) -> Flight {
        self.flight
    }

    pub(crate) const fn payload(&self) -> Payload {
        self.payload
    }

    /// Counts `flown` meters flown in all.
    pub(crate) fn fly_to(&mut self, flown: Num) {
        match &mut self.flight {
            Flight::Homing { flown: at, .. } => *at = flown,
            Flight::Line {
                flown: at, range, ..
            } => {
                debug_assert!(flown <= *range, "a line is flown within its range");
                *at = flown;
            }
        }
    }
}

impl SimComponent for Projectile {
    const NAME: &'static str = "projectiles.projectile";

    // A projectile of a type with no flight, of an action the book lacks or at a rank past its
    // ranks, or of a damage kind the mode lacks, has no rules for its flight or its hit.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let flies = world
            .get::<UnitType>(entity)
            .zip(world.get_resource::<ByType<ProjectileSpec>>())
            .is_some_and(|(&unit_type, specs)| specs.get(unit_type).is_some());
        let book = world.get_resource::<ActionBook>();
        let carries = match self.payload {
            Payload::Attack {
                action, rank, kind, ..
            } => {
                book.and_then(|book| book.get(action))
                    .is_some_and(|action| action.has_rank(rank))
                    && world
                        .get_non_send::<View>()
                        .is_none_or(|view| view.has_damage_kind(kind))
            }
            Payload::Action { action, rank, .. } => book
                .and_then(|book| book.get(action))
                .is_some_and(|action| action.has_rank(rank)),
        };
        flies && carries
    }
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
