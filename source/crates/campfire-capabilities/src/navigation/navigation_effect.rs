use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, SimWorld, StableId};

use crate::actions::capability_does_spec::CapabilityDoesSpec;
use crate::actions::effect_data::EffectTo;
use crate::deliveries::teleported::Teleported;
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::navigation_column::NavigationColumn;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::walker::Walker;
use crate::scripts::effects::Effect;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::frame::Frame;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::{DashDelivery, DashTo, ForcedMove};
use crate::units::map_book::MapBook;
use crate::units::view::View;

/// A forced move a call queued, of a living unit that walks: a dash or a knock back, which starts
/// in place of any under way, or a teleport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigationEffect {
    /// A dash of `step` a tick to `to`, the delivery `delivers` when it is one.
    Dash {
        id: StableId,
        to: DashTo,
        step: Num,
        delivers: Option<DashDelivery>,
    },
    /// A knock back `distance` away from `from` over `ticks`.
    KnockBack {
        id: StableId,
        from: Position,
        distance: Num,
        ticks: Ticks,
    },
    Teleport {
        id: StableId,
        to: Position,
    },
}

impl NavigationEffect {
    /// A dash of `id`, which walks, to `to` at `speed` meters a second, which `view` sees: a
    /// step a tick of `speed` over the tick rate, rounded once, more than 0. A call that delivers
    /// an instant action, its `on_resolve`, makes the dash that delivery, as `frame` says.
    pub(crate) fn dash(
        view: &View,
        frame: &Frame,
        id: StableId,
        to: DashTo,
        speed: Num,
    ) -> Result<NavigationEffect, ApiError> {
        NavigationEffect::walker(view, id)?;
        let hz = i64::from(view.rate().hz().get());
        let step = speed
            .checked_div_int(hz)
            .filter(|&step| step > Num::ZERO)
            .ok_or(ApiError::NotASpeed)?;
        Ok(NavigationEffect::Dash {
            id,
            to,
            step,
            delivers: frame.dash_delivers(),
        })
    }

    /// A knock back of `id`, which walks, `distance` away from `from`, more than 0 and within
    /// the world's bound, over `ms`, more than 0, rounded up to ticks.
    pub(crate) fn knock_back(
        view: &View,
        id: StableId,
        from: Position,
        distance: Num,
        ms: i64,
    ) -> Result<NavigationEffect, ApiError> {
        NavigationEffect::walker(view, id)?;
        if distance <= Num::ZERO || distance > Position::BOUND {
            return Err(ApiError::NotADistance);
        }
        let ticks = view.lasting(ms)?;
        Ok(NavigationEffect::KnockBack {
            id,
            from,
            distance,
            ticks,
        })
    }

    /// A teleport of `id`, which walks, to `to`.
    pub(crate) fn teleport(
        view: &View,
        id: StableId,
        to: Position,
    ) -> Result<NavigationEffect, ApiError> {
        NavigationEffect::walker(view, id)?;
        Ok(NavigationEffect::Teleport { id, to })
    }

    /// Queues a listed forced move of `id`, whose other unit is the acting unit or `reached`,
    /// the unit the list reached, by the rules of the script's calls.
    pub(crate) fn queue_listed(
        does: CapabilityDoesSpec,
        id: StableId,
        reached: Option<StableId>,
        frame: &mut Frame,
        view: &View,
    ) -> Result<(), CallError> {
        let acting = frame.acting();
        let other = |to: EffectTo| match to {
            EffectTo::Reached => Ok(reached.expect(
                "the load lets a move name the reached unit only in a list that reaches one",
            )),
            EffectTo::Source => acting.ok_or(ApiError::NoActingUnit),
        };
        let effect = match does {
            CapabilityDoesSpec::Dash { to, speed } => other(to).and_then(|to| {
                NavigationEffect::dash(view, frame, id, DashTo::Unit(to), speed.number(frame))
            }),
            CapabilityDoesSpec::KnockBack { from, distance, ms } => other(from).and_then(|from| {
                let from = view.row(from).ok_or(ApiError::NoActingUnit)?.pos;
                // Whole, as the load checked, so the floor is exact.
                let ms = ms.number(frame).floor();
                NavigationEffect::knock_back(view, id, from, distance.number(frame), ms)
            }),
            _ => unreachable!("navigation queues only its own listed effects"),
        };
        frame.effects.push(effect.map_err(CallError::Api)?);
        Ok(())
    }

    /// Fails unless `id` walks: only a walker moves.
    fn walker(view: &View, id: StableId) -> Result<(), ApiError> {
        if !NavigationColumn::walks(view, id) {
            return Err(ApiError::NoWalker);
        }
        Ok(())
    }

    /// Puts the unit of `entity` at `pos`, at once: it ends its forced move and walks its route
    /// again from there, as a teleport and a box that spawns over it put a unit.
    pub(super) fn put(world: &mut World, entity: Entity, pos: Position, now: Tick) {
        let mut moved = world.entity_mut(entity);
        *moved.get_mut::<Position>().expect("a unit has a place") = pos;
        moved.remove::<ForcedMove>();
        if let Ok((destination, mut route, mut progress)) =
            moved.get_components_mut::<(&Destination, &mut Route, &mut Progress)>()
        {
            route.ask_again(destination, &mut progress, now);
        }
    }
}

impl Effect for NavigationEffect {
    // A unit gone or dead since the call moves no more.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        let id = match self {
            NavigationEffect::Dash { id, .. }
            | NavigationEffect::KnockBack { id, .. }
            | NavigationEffect::Teleport { id, .. } => id,
        };
        let Some(entity) = world.entity_of(id) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let forced = match self {
            NavigationEffect::Dash {
                to, step, delivers, ..
            } => ForcedMove::Dash { to, step, delivers },
            NavigationEffect::KnockBack {
                from,
                distance,
                ticks,
                ..
            } => {
                let pos = *world.get::<Position>(entity).expect("a unit has a place");
                ForcedMove::KnockBack {
                    to: knock_back_end(pos, from, distance),
                    left: ticks.get(),
                }
            }
            NavigationEffect::Teleport { to, .. } => {
                teleport(world, entity, id, to, now);
                return;
            }
        };
        world.entity_mut(entity).insert(forced);
    }
}

/// Where a knock back `distance` away from `from` takes a unit at `pos`: straight away on the
/// ground plane, each component of the direction rounded once, along x from `from` itself, as
/// collision parts two on one spot.
fn knock_back_end(pos: Position, from: Position, distance: Num) -> Vec3 {
    let away =
        from.ground_offset(pos)
            .normalized()
            .unwrap_or(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO));
    let offset = away
        .checked_mul(distance)
        .expect("a distance within the bound scales a unit vector");
    pos.get()
        .checked_add(offset)
        .expect("a place and a distance within the bound add")
}

/// Puts the unit of `entity`, `id`, at `to`, taken to the nearest point of the bounds, or, where
/// its walker may not stand on the pathing grid, at the center of the nearest cell it may stand
/// in, at `to`'s height; on a grid with no such cell, it stays. It ends its forced move and walks
/// its route again from there, and every delivery that homes on it loses it.
fn teleport(world: &mut World, entity: Entity, id: StableId, to: Position, now: Tick) {
    let mut pos = world.resource::<MapBook>().bounds.clamp(to);
    if let Some(grid) = world.get_resource::<PathingGrid>()
        && PathingGrid::serves(world, entity)
    {
        let walkable = Walkable::of(
            grid.clearance(Walker::walking(world.get::<Body>(entity))),
            world.resource::<BodyIndex>(),
        );
        let Some(open) = world.resource::<RoutePlanner>().stand_at(walkable, pos) else {
            return;
        };
        pos = open;
    }
    NavigationEffect::put(world, entity, pos, now);
    world.trigger(Teleported { id });
}
