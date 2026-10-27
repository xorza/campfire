use std::num::NonZeroI64;

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Query, Res};
use campfire_declarations::ById;
use campfire_math::{Num, SinCos, Vec3};
use campfire_sim::{MatchRate, Position};
use campfire_units::{
    Dead, Destination, Facing, ForcedMove, MoveStep, OnWings, StatusTags, TurnStep,
};
use campfire_values::{Block, Motion, UnitTags, UnitType};

use crate::arrived_at::ArrivedAt;
use crate::flight_data::FlightData;
use crate::held_place::HeldPlace;
use crate::progress::Progress;
use crate::route::RouteMut;

/// Navigation's winged fliers in the air, which never stand (04-capabilities/navigation.md, A jet
/// flies on wings).
#[derive(Debug)]
pub(crate) struct Wings;

/// The turn from a winged flier's bearing to the place it holds to the point of its circle it
/// flies toward, 157.5°, as Zero Hour's `maintainCurrentPositionWings` aims π − π/8 round.
const CIRCLE_AIM: Num = Num::from_bits(315 << (Num::FRAC_BITS - 1));

impl Wings {
    /// Flies each living flier on wings in the air, `on_wings`, after the walkers walk: it turns
    /// toward its goal at its turn step and moves along its facing, never standing. Its goal is its
    /// route's next waypoint, at its speed, which it passes within its circling radius, and past
    /// its last it has arrived, as a walker does, and holds that goal's place; with no destination,
    /// or one its tags stop, its goal is the point of its circle round the place it holds,
    /// `CIRCLE_AIM` round from its bearing to that place, at its least speed; with a route the
    /// planner has yet to give, it flies on along its facing.
    #[expect(
        clippy::type_complexity,
        reason = "a system takes each part it reads and writes"
    )]
    pub(crate) fn fly(
        (rate, book): (Res<'_, MatchRate>, Res<'_, ById<UnitType, FlightData>>),
        mut fliers: Query<
            '_,
            '_,
            (
                (&UnitType, &mut Position, &mut Facing, &mut HeldPlace),
                (&mut Destination, RouteMut, &mut ArrivedAt, &mut Progress),
                (&MoveStep, Option<&TurnStep>),
                (Option<&StatusTags>, Option<&UnitTags>),
            ),
            (Without<Dead>, Without<ForcedMove>),
        >,
    ) {
        let hz = Num::int(NonZeroI64::from(rate.hz()).get());
        for (
            (&unit_type, mut position, mut facing, mut held),
            (mut destination, mut route, mut arrived, mut progress),
            (step, turn),
            (status, tags),
        ) in &mut fliers
        {
            if !OnWings::holds(status) {
                continue;
            }
            let flight = book
                .get(unit_type)
                .expect("a winged flier's type has its flight");
            let wings = flight.wings.expect("a winged flier's flight has its wings");
            let at = position.get();
            let blocked = tags
                .copied()
                .unwrap_or_default()
                .blocks(Motion::Own, Block::Move);
            let walks = destination.get().is_some() && !blocked;
            let (goal, speed) = match route.read().ahead().first() {
                Some(&waypoint) if walks => (Some(waypoint), step.get()),
                _ if walks && route.asked().is_some() => (None, step.get()),
                _ => (
                    Some(Self::circle_point(at, held.get(), wings.circling, *facing)),
                    wings.least_speed / hz,
                ),
            };
            let way = goal.and_then(|goal| Facing::along([goal.x - at.x, goal.z - at.z]));
            if let Some(way) = way {
                facing.set_if_neq(facing.toward(way, turn.copied()).facing);
            }
            let along = SinCos::of_degrees(facing.degrees());
            let moved = Vec3::new(at.x + along.cos * speed, at.y, at.z - along.sin * speed);
            position.set_if_neq(Position::from(moved));
            if !walks {
                continue;
            }
            ArrivedAt::set(&mut arrived, None);
            while let Some(&waypoint) = route.read().ahead().first() {
                if !moved.within_ground(waypoint, wings.circling) {
                    break;
                }
                route.advance();
                progress.restart();
            }
            if route.read().ahead().is_empty() && route.asked().is_none() {
                let goal = route.goal();
                Destination::route_to(&mut destination, None);
                ArrivedAt::set(&mut arrived, goal);
                if let Some(goal) = goal {
                    held.set_if_neq(HeldPlace::new(goal));
                }
                if route.is_reached() {
                    route.clear();
                    progress.restart();
                }
            }
        }
    }

    /// The point a winged flier at `at` flies toward to circle `center` at `radius`: the point of
    /// that circle `CIRCLE_AIM` round from its bearing to the center, its facing's for a flier
    /// right over it.
    fn circle_point(at: Vec3, center: Vec3, radius: Num, facing: Facing) -> Vec3 {
        let bearing = Facing::along([center.x - at.x, center.z - at.z]).unwrap_or(facing);
        let aim = SinCos::of_degrees(bearing.degrees() + CIRCLE_AIM);
        Vec3::new(
            center.x + aim.cos * radius,
            at.y,
            center.z - aim.sin * radius,
        )
    }
}
