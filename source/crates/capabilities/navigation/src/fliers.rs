use std::num::NonZeroI64;

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Query, Res};
use campfire_declarations::ById;
use campfire_math::{Num, Vec3};
use campfire_sim::{MatchRate, Position};
use campfire_units::{Dead, FlightGoal, MapBook, OnWings, StatusTags};
use campfire_values::UnitType;

use crate::Navigation;
use crate::altitude::Altitude;
use crate::flight_data::FlightData;

/// Navigation's fliers: units of a type of `flight`, whose height it alone writes.
#[derive(Debug)]
pub(crate) struct Fliers;

impl Fliers {
    /// Flies each living flier, after every move of the tick: its height in the world moves toward
    /// the ground under it plus its type's height, or toward the ground for one its `FlightGoal`
    /// holds there, by its climb a tick, never below the ground, and its position takes the height
    /// above the ground that gives; it holds the engine tag `air_target` while that height passes
    /// its type's airborne height, and a winged flier `on_wings` too (helicopters.md, A flier holds
    /// a height; helicopters.md, Airborne is a tag of height; jets.md, A jet flies on wings).
    pub(crate) fn fly(
        (map, rate, book): (
            Res<'_, MapBook>,
            Res<'_, MatchRate>,
            Res<'_, ById<UnitType, FlightData>>,
        ),
        mut fliers: Query<
            '_,
            '_,
            (
                &UnitType,
                &mut Position,
                &mut Altitude,
                &mut StatusTags,
                &FlightGoal,
            ),
            Without<Dead>,
        >,
    ) {
        let hz = NonZeroI64::from(rate.hz()).get();
        for (&unit_type, mut position, mut altitude, mut status, &goal) in &mut fliers {
            let flight = *book.get(unit_type).expect("a flier's type has its flight");
            let at = position.get();
            let ground = map.point(Vec3::new(at.x, Num::ZERO, at.z)).y;
            let goal = match goal {
                FlightGoal::Height => ground + flight.height,
                FlightGoal::Ground => ground,
            };
            let climb = flight.climb / hz;
            let now = altitude.get();
            let next = if now < goal {
                (now + climb).min(goal)
            } else {
                (now - climb).max(goal)
            };
            let next = next.max(ground);
            altitude.set_if_neq(Altitude::new(next));
            let height = next - ground;
            position.set_if_neq(Position::from(Vec3::new(at.x, height, at.z)));
            let airborne = height > flight.airborne;
            let target = status
                .turned(Navigation::AIR_TARGET, airborne)
                .turned(OnWings::TAG, airborne && flight.wings.is_some());
            status.set_if_neq(target);
        }
    }
}
