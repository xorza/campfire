use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, Position, StableId};

use crate::actions::effect_data::EffectTo;
use crate::actions::effect_lists::Does;
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::navigation_column::NavigationColumn;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::walker::Walker;
use crate::projectiles::projectile::Projectile;
use crate::scripts::effects::Effect;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::frame::Frame;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::{DashDelivery, DashTo, ForcedMove};
use crate::units::script_view::View;
use crate::values::bounds::Bounds;

/// A forced move a call queued, of a living unit that walks: a dash or a knock back, which starts
/// in place of any under way, or a teleport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigationEffect {
    /// A dash of `step` a tick to `to`, the delivery `delivers` when it is one.
    Dash {
        unit: StableId,
        to: DashTo,
        step: Num,
        delivers: Option<DashDelivery>,
    },
    /// A knock back `distance` away from `from` over `ticks`.
    KnockBack {
        unit: StableId,
        from: Position,
        distance: Num,
        ticks: Ticks,
    },
    Teleport {
        unit: StableId,
        to: Position,
    },
}

impl NavigationEffect {
    /// A dash of `unit`, which walks, to `to` at `speed` meters a second, which `view` sees: a
    /// step a tick of `speed` over the tick rate, rounded once, more than 0. A call that delivers
    /// an instant action, its `on_resolve`, makes the dash that delivery, as `frame` says.
    pub(crate) fn dash(
        view: &View,
        frame: &Frame,
        unit: StableId,
        to: DashTo,
        speed: Num,
    ) -> Result<NavigationEffect, ApiError> {
        NavigationEffect::walker(view, unit)?;
        let hz = i64::from(view.rate().hz().get());
        let step = speed
            .checked_div_int(hz)
            .filter(|&step| step > Num::ZERO)
            .ok_or(ApiError::NotASpeed)?;
        Ok(NavigationEffect::Dash {
            unit,
            to,
            step,
            delivers: frame.dash_delivers(),
        })
    }

    /// A knock back of `unit`, which walks, `distance` away from `from`, more than 0 and within
    /// the world's bound, over `ms`, more than 0, rounded up to ticks.
    pub(crate) fn knock_back(
        view: &View,
        unit: StableId,
        from: Position,
        distance: Num,
        ms: i64,
    ) -> Result<NavigationEffect, ApiError> {
        NavigationEffect::walker(view, unit)?;
        if distance <= Num::ZERO || distance > Position::BOUND {
            return Err(ApiError::NotADistance);
        }
        let ticks = view.lasting(ms)?;
        Ok(NavigationEffect::KnockBack {
            unit,
            from,
            distance,
            ticks,
        })
    }

    /// A teleport of `unit`, which walks, to `to`.
    pub(crate) fn teleport(
        view: &View,
        unit: StableId,
        to: Position,
    ) -> Result<NavigationEffect, ApiError> {
        NavigationEffect::walker(view, unit)?;
        Ok(NavigationEffect::Teleport { unit, to })
    }

    /// Queues a listed forced move of `unit`, whose other unit is the acting unit or `reached`,
    /// the unit the list reached, by the rules of the script's calls.
    pub(crate) fn queue_listed(
        does: Does,
        unit: StableId,
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
            Does::Dash { to, speed } => other(to).and_then(|to| {
                NavigationEffect::dash(view, frame, unit, DashTo::Unit(to), speed.number(frame))
            }),
            Does::KnockBack { from, distance, ms } => other(from).and_then(|from| {
                let from = view.row(from).ok_or(ApiError::NoActingUnit)?.pos;
                // Whole, as the load checked, so the floor is exact.
                let ms = ms.number(frame).floor();
                NavigationEffect::knock_back(view, unit, from, distance.number(frame), ms)
            }),
            _ => unreachable!("navigation queues only its own listed effects"),
        };
        frame.effects.push(effect.map_err(CallError::Api)?);
        Ok(())
    }

    /// Fails unless `unit` walks: only a walker moves.
    fn walker(view: &View, unit: StableId) -> Result<(), ApiError> {
        if !NavigationColumn::walks(view, unit) {
            return Err(ApiError::NoWalker);
        }
        Ok(())
    }
}

impl Effect for NavigationEffect {
    // A unit gone or dead since the call moves no more.
    fn apply(self, world: &mut World, _: &mut Frame, now: Tick) {
        let unit = match self {
            NavigationEffect::Dash { unit, .. }
            | NavigationEffect::KnockBack { unit, .. }
            | NavigationEffect::Teleport { unit, .. } => unit,
        };
        let Some(entity) = world.resource::<EntityIndex>().get(unit) else {
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
                let at = *world.get::<Position>(entity).expect("a unit has a place");
                ForcedMove::KnockBack {
                    to: knock_back_end(at, from, distance),
                    left: ticks.get(),
                }
            }
            NavigationEffect::Teleport { to, .. } => {
                teleport(world, entity, unit, to, now);
                return;
            }
        };
        world.entity_mut(entity).insert(forced);
    }
}

/// Where a knock back `distance` away from `from` takes a unit at `at`: straight away on the
/// ground plane, each component of the direction rounded once, along x from `from` itself, as
/// collision parts two on one spot.
fn knock_back_end(at: Position, from: Position, distance: Num) -> Vec3 {
    let away =
        from.ground_offset(at)
            .normalized()
            .unwrap_or(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO));
    let offset = away
        .checked_scale(distance)
        .expect("a distance within the bound scales a unit vector");
    at.get()
        .checked_add(offset)
        .expect("a place and a distance within the bound add")
}

impl NavigationEffect {
    /// Puts the unit of `entity` at `place`, at once: it ends its forced move and walks its route
    /// again from there, as a teleport and a box that spawns over it put a unit.
    pub(crate) fn put(world: &mut World, entity: Entity, place: Position, now: Tick) {
        let mut moved = world.entity_mut(entity);
        *moved.get_mut::<Position>().expect("a unit has a place") = place;
        moved.remove::<ForcedMove>();
        let mut walkers = world.query::<(&Destination, &mut Route, &mut Progress)>();
        if let Ok((destination, mut route, mut progress)) = walkers.get_mut(world, entity) {
            route.ask_again(destination, &mut progress, now);
        }
    }
}

/// Puts the unit of `entity`, `unit`, at `to`, taken to the nearest point of the bounds, or, where
/// its walker may not stand on the pathing grid, at the center of the nearest cell it may stand
/// in, at `to`'s height; on a grid with no such cell, it stays. It ends its forced move and walks
/// its route again from there, and every homing projectile on it loses it.
fn teleport(world: &mut World, entity: Entity, unit: StableId, to: Position, now: Tick) {
    let mut place = world.resource::<Bounds>().clamp(to);
    if let Some(grid) = world.get_resource::<PathingGrid>()
        && PathingGrid::serves(world, entity)
    {
        let walkable = Walkable {
            clearance: grid.clearance(Walker::walking(world.get::<Body>(entity))),
            statics: world.resource::<BodyIndex>(),
            short: None,
        };
        let Some(open) = world.resource::<RoutePlanner>().stand_at(walkable, place) else {
            return;
        };
        place = open;
    }
    NavigationEffect::put(world, entity, place, now);
    let mut projectiles = world.query::<&mut Projectile>();
    for mut projectile in projectiles.iter_mut(world) {
        if projectile.homes_on(unit) {
            projectile.disjoint();
        }
    }
}
