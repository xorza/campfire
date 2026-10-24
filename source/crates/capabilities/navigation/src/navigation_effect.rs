use std::num::NonZeroI64;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_deliveries::DashDelivering;
use campfire_math::{Num, Rounding, Vec3};
use campfire_script::{
    ApiError, CapabilityRefusal, Effect, EffectKind, Frame, ScriptLasting, View,
};
use campfire_sim::{Position, SimWorld, StableId, UnitDues};
use campfire_units::{Body, DashTo, Dead, Destination, ForcedMove, MapBook, Teleported};
use campfire_values::DashDelivery;

use crate::body_index::BodyIndex;
use crate::error::NavigationRefusal;
use crate::navigation_column::NavigationColumn;
use crate::pathing_grid::PathingGrid;
use crate::progress::Progress;
use crate::route::{Route, RouteMut};
use crate::route_planner::{RoutePlanner, Walkable};
use crate::walker::Walker;

/// A forced move a call queued, of a living unit that walks: a dash or a knock back, which starts
/// in place of any under way, or a teleport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationEffect {
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
        from: Vec3,
        distance: Num,
        ticks: Ticks,
    },
    Teleport {
        id: StableId,
        to: Vec3,
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
    ) -> Result<Self, ApiError> {
        Self::walker(view, id)?;
        let hz = NonZeroI64::from(view.rate().hz()).get();
        let step = speed
            .checked_div_int(hz, Rounding::NearestEven)
            .filter(|&step| step > Num::ZERO)
            .ok_or_else(|| NavigationRefusal::NotASpeed.refused())?;
        Ok(Self::Dash {
            id,
            to,
            step,
            delivers: frame.dash_delivers(),
        })
    }

    /// A knock back of `id`, which walks, `distance` away from `from`, more than 0 and within
    /// the world's bound, over `lasting`.
    pub(crate) fn knock_back(
        view: &View,
        id: StableId,
        from: Vec3,
        distance: Num,
        lasting: ScriptLasting,
    ) -> Result<Self, ApiError> {
        Self::walker(view, id)?;
        if distance <= Num::ZERO || distance > Vec3::BOUND {
            return Err(NavigationRefusal::NotADistance.refused());
        }
        let ticks = lasting.get();
        Ok(Self::KnockBack {
            id,
            from,
            distance,
            ticks,
        })
    }

    /// A teleport of `id`, which walks, to `to`.
    pub(crate) fn teleport(view: &View, id: StableId, to: Vec3) -> Result<Self, ApiError> {
        Self::walker(view, id)?;
        Ok(Self::Teleport { id, to })
    }

    /// Fails unless `id` walks: only a walker moves.
    fn walker(view: &View, id: StableId) -> Result<(), ApiError> {
        if !NavigationColumn::walks(view, id) {
            return Err(NavigationRefusal::NoWalker.refused());
        }
        Ok(())
    }

    /// Puts the unit of `entity` at `position`, at once: it ends its forced move and walks its
    /// route again from there, as a teleport and a box that spawns over it put a unit.
    pub(super) fn put(world: &mut World, entity: Entity, position: Vec3, now: Tick) {
        let id = *world.get::<StableId>(entity).expect("a unit has an id");
        UnitDues::<Route>::with(world, id, entity, |world, dues| {
            let mut moved = world.entity_mut(entity);
            *moved.get_mut::<Position>().expect("a unit has a place") = Position::from(position);
            moved.remove::<(ForcedMove, DashDelivering)>();
            if let Ok((destination, mut route, mut progress)) =
                moved.get_components_mut::<(&Destination, RouteMut, &mut Progress)>()
            {
                route.ask_again(destination.get(), &mut progress, now, dues);
            }
        });
    }
}

impl Effect for NavigationEffect {
    const KIND: EffectKind = EffectKind::Change;

    // A unit gone or dead since the call moves no more.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        let id = match self {
            Self::Dash { id, .. } | Self::KnockBack { id, .. } | Self::Teleport { id, .. } => id,
        };
        let Some(entity) = world.entity_of(id) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let forced = match self {
            Self::Dash {
                to, step, delivers, ..
            } => {
                let forced = ForcedMove::Dash { to, step };
                let mut moved = world.entity_mut(entity);
                match delivers {
                    Some(delivers) => moved.insert((forced, DashDelivering::new(delivers))),
                    None => moved.insert(forced).remove::<DashDelivering>(),
                };
                return;
            }
            Self::KnockBack {
                from,
                distance,
                ticks,
                ..
            } => {
                let position = *world.get::<Position>(entity).expect("a unit has a place");
                ForcedMove::KnockBack {
                    to: knock_back_end(position.get(), from, distance),
                    left: ticks.get(),
                }
            }
            Self::Teleport { to, .. } => {
                teleport(world, entity, id, to, now);
                return;
            }
        };
        world
            .entity_mut(entity)
            .insert(forced)
            .remove::<DashDelivering>();
    }
}

/// Where a knock back `distance` away from `from` takes a unit at `position`: straight away on the
/// ground plane, each component of the direction rounded once, along x from `from` itself, as
/// collision parts two on one spot.
fn knock_back_end(position: Vec3, from: Vec3, distance: Num) -> Vec3 {
    let away = from
        .ground_offset(position)
        .normalized()
        .unwrap_or(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO));
    let offset = away
        .checked_mul(distance)
        .expect("a distance within the bound scales a unit vector");
    position
        .checked_add(offset)
        .expect("a place and a distance within the bound add")
}

/// Puts the unit of `entity`, `id`, at `to`, taken to the nearest point of the bounds, or, where
/// its walker may not stand on the pathing grid, at the center of the nearest cell it may stand
/// in, at `to`'s height; on a grid with no such cell, it stays. It ends its forced move and walks
/// its route again from there, and every delivery that homes on it loses it.
fn teleport(world: &mut World, entity: Entity, id: StableId, to: Vec3, now: Tick) {
    let mut position = world.resource::<MapBook>().bounds.clamp(to);
    let clearance = world.get_resource::<PathingGrid>().and_then(|grid| {
        let walker = Walker::of(world.get::<Body>(entity))?;
        grid.serving(walker)
    });
    if let Some(clearance) = clearance {
        let walkable = Walkable::of(clearance, world.resource::<BodyIndex>());
        let Some(open) = world
            .resource::<RoutePlanner>()
            .stand_at(walkable, position)
        else {
            return;
        };
        position = open;
    }
    NavigationEffect::put(world, entity, position, now);
    world.trigger(Teleported { id });
}
