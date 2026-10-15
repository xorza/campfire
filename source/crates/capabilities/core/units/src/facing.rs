use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::KindSpawn;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentPredicted};
use serde::{Deserialize, Serialize};

use crate::turn_step::TurnStep;
use crate::unit_entity::UnitEntity;

/// The way a unit faces, in degrees from `x` towards `−z`, counter-clockwise seen from above,
/// taken modulo 360: its spawn's angle, a placed unit's or a build order's, as its box body turns
/// by it; a walker's turns toward the way it walks (design 17, D2).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Facing(Num);

/// A facing turned toward a way, and whether it then faces within `Facing::WITHIN` of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    pub facing: Facing,
    pub faces: bool,
}

/// A full turn, in a `Num`'s bits.
const FULL: i64 = 360 << Num::FRAC_BITS;

/// Half a turn, in a `Num`'s bits.
const HALF: i64 = 180 << Num::FRAC_BITS;

impl Facing {
    /// `angle` degrees, taken modulo 360.
    pub const fn of(angle: Num) -> Facing {
        Facing(Num::from_bits(angle.to_bits().rem_euclid(FULL)))
    }

    /// The angle in degrees the unit of `at` faces, which its spawn gives; 0 for an entity that
    /// faces none.
    pub(crate) fn angle_of<H>(at: &KindSpawn<'_, H>) -> Num {
        let facing = at.world().get::<Facing>(at.entity());
        facing.map_or(Num::ZERO, |facing| facing.degrees())
    }

    /// The angle within which a walker faces its way and steps, 11.25°: Dota 2 lets a unit move
    /// once it faces within 0.2 radians of its way, about 11.46°, and this is the nearest angle
    /// of few bits.
    pub const WITHIN: Num = Num::from_bits(45 << (Num::FRAC_BITS - 2));

    /// Its degrees, at least 0 and below 360.
    pub const fn degrees(self) -> Num {
        self.0
    }

    /// The facing along `way`, its `x` and `z` on the ground plane: its angle in degrees,
    /// rounded once; `None` for a way of no length.
    pub fn along([x, z]: [Num; 2]) -> Option<Facing> {
        if x == Num::ZERO && z == Num::ZERO {
            return None;
        }
        Some(Facing::of((-z).atan2_degrees(x)))
    }

    /// It turned toward `way` the shorter way round, counter-clockwise on a tie: by at most
    /// `step`'s degrees, or at once with no step; and whether it then faces within
    /// `Facing::WITHIN` of the way.
    pub fn toward(self, way: Facing, step: Option<TurnStep>) -> Turn {
        // The turn to the way, from just past −180° to 180°.
        let mut left = Num::from_bits((way.0.to_bits() - self.0.to_bits()).rem_euclid(FULL));
        if left.to_bits() > HALF {
            left = Num::from_bits(left.to_bits() - FULL);
        }
        let turn = step.map_or(left, |step| left.clamp(-step.get(), step.get()));
        let rest = left - turn;
        Turn {
            facing: Facing::of(self.0 + turn),
            faces: -Facing::WITHIN <= rest && rest <= Facing::WITHIN,
        }
    }
}

impl SimComponent for Facing {
    const NAME: &'static str = "units.facing";
    type Host = UnitEntity;

    fn check(&self, _: &World, _: Entity) -> bool {
        (0..FULL).contains(&self.0.to_bits())
    }
}

impl Replication for Facing {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_facing_is_its_angle_modulo_360() {
        let degrees = |angle: i32| Facing::of(Num::int(angle.into())).degrees();
        assert_eq!(degrees(0), Num::ZERO);
        assert_eq!(degrees(90), Num::int(90));
        assert_eq!(degrees(360), Num::ZERO);
        assert_eq!(degrees(-90), Num::int(270));
        assert_eq!(degrees(725), Num::int(5));
        // The least step below 0 is the greatest below 360.
        let least = Facing::of(Num::from_bits(-1)).degrees();
        assert_eq!(least.to_bits(), FULL - 1);
        let world = World::new();
        let entity = Entity::PLACEHOLDER;
        assert!(Facing::of(Num::from_bits(-1)).check(&world, entity));
        assert!(!Facing(Num::int(360)).check(&world, entity));
        assert!(!Facing(Num::from_bits(-1)).check(&world, entity));
    }

    #[test]
    fn a_way_gives_its_angle_and_a_turn_takes_the_shorter_side_at_most_its_step() {
        let degrees = |way: [i64; 2]| Facing::along(way.map(Num::int)).map(Facing::degrees);
        // Along `x` is 0°, along `−z` 90°, along `−x` 180° and along `+z` 270°; no way, none.
        assert_eq!(degrees([1, 0]), Some(Num::ZERO));
        assert_eq!(degrees([0, -1]), Some(Num::int(90)));
        assert_eq!(degrees([-3, 0]), Some(Num::int(180)));
        assert_eq!(degrees([0, 5]), Some(Num::int(270)));
        assert_eq!(degrees([0, 0]), None);
        // Toward `x` and `−z` alike: 45°.
        assert_eq!(degrees([2, -2]), Some(Num::int(45)));
        let facing = |angle: i64| Facing::of(Num::int(angle));
        let three = TurnStep::new(Num::int(3));
        let toward = |from: i64, way: i64| facing(from).toward(facing(way), three);
        // 3° a step: from 0° toward 90°, to 3°, 87° short; from 80°, to 83°, 7° short, within
        // 11.25°; from 89°, the 1° left.
        assert_eq!(
            toward(0, 90),
            Turn {
                facing: facing(3),
                faces: false
            }
        );
        assert_eq!(
            toward(80, 90),
            Turn {
                facing: facing(83),
                faces: true
            }
        );
        assert_eq!(
            toward(89, 90),
            Turn {
                facing: facing(90),
                faces: true
            }
        );
        // The shorter side: from 10° toward 350°, back to 7°; half a turn, counter-clockwise.
        assert_eq!(
            toward(10, 350),
            Turn {
                facing: facing(7),
                faces: false
            }
        );
        assert_eq!(
            toward(0, 180),
            Turn {
                facing: facing(3),
                faces: false
            }
        );
        assert_eq!(
            toward(180, 0),
            Turn {
                facing: facing(183),
                faces: false
            }
        );
        // 11.25° short faces; a bit more does not.
        let short = Facing::of(Num::int(90) - Facing::WITHIN);
        let zero = TurnStep::new(Num::ZERO);
        assert!(short.toward(facing(90), zero).faces);
        let past = Facing::of(Num::int(90) - Facing::WITHIN - Num::EPSILON);
        assert!(!past.toward(facing(90), zero).faces);
        // With no step, at once.
        assert_eq!(
            facing(10).toward(facing(250), None),
            Turn {
                facing: facing(250),
                faces: true
            }
        );
    }
}
