use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_actions::Targets;
use campfire_common::Ticks;
use campfire_declare::ByType;
use campfire_deliveries::{Delivered, Deliveries, DeliverySpawner, Reached};
use campfire_geometry::Shape;
use campfire_math::Num;
use campfire_sim::{Keyed, Ordered, Position, SimTick, StableId};
use campfire_units::UnitsNear;
use campfire_values::{Hit, Team, UnitType};

use crate::area::Area;
use crate::area_launches::{AreaLaunch, AreaLaunches};
use crate::area_spec::AreaSpec;
use crate::area_trigger::AreaTrigger;

/// The life of the areas: each lands from its launch, then triggers and ends.
#[derive(Debug)]
pub struct AreaLife;

impl AreaLife {
    /// Triggers each area whose delay ended, and ends each whose time is up, in the order of their
    /// stable ids. A trigger reaches each living unit its type's `affects` selects whose body comes
    /// within its radius, whose tags block it as a target or not, by stable id, each with its
    /// action's `on_hit`, and the area's trigger goes; an end, of an area with no trigger to come,
    /// runs `on_end`, and the area despawns after the hooks. Each hit is at the area's centre, of
    /// no distance or direction.
    pub(super) fn trigger(
        targets: Targets<'_, '_>,
        (specs, tick): (Res<'_, ByType<AreaSpec>>, Res<'_, SimTick>),
        mut deliveries: ResMut<'_, Deliveries>,
        mut commands: Commands<'_, '_>,
        areas: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Position,
                &Team,
                &UnitType,
                &Area,
                Option<&AreaTrigger>,
            ),
        >,
        (mut order, mut reached, mut near): (
            Local<'_, Ordered>,
            Local<'_, Vec<StableId>>,
            UnitsNear<'_, '_>,
        ),
    ) {
        let now = tick.start();
        let triggers = areas
            .iter()
            .any(|(.., trigger)| trigger.is_some_and(|trigger| trigger.get() <= now));
        let grid = triggers.then(|| near.current());
        let placed = areas.iter().map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { id, entity } in order.sort(placed) {
            let (_, _, &pos, &team, &unit_type, area, trigger) =
                areas.get(entity).expect("an area in the order");
            let triggers_at = trigger.map(|trigger| trigger.get());
            let hit = Hit {
                delivery: Some(id),
                target: area.aimed(),
                pos,
                distance: Num::ZERO,
                direction: None,
            };
            let by = area.by();
            let delivered = |reach| Delivered { by, reach, hit };
            if triggers_at.is_some_and(|at| at <= now) {
                let spec = specs.get(unit_type).expect("an area's type has a spec");
                reached.clear();
                let grid = grid.expect("a trigger finds the units near");
                grid.visit_near(pos, spec.radius, |body| {
                    let Some(unit) = targets.living_at(body.key) else {
                        return;
                    };
                    let relation = targets.relation(team, unit.team);
                    if spec.affects.selects(relation, unit.tags)
                        && targets.reaches(pos, Shape::POINT, spec.radius, &unit)
                    {
                        reached.push(unit.id);
                    }
                });
                reached.sort_unstable();
                for &unit in &reached {
                    deliveries.deliver(delivered(Reached::Hit(unit)));
                }
                commands.entity(entity).remove::<AreaTrigger>();
            }
            if triggers_at.is_none_or(|at| at <= now) && area.ends_at() <= now {
                deliveries.deliver(delivered(Reached::End));
                deliveries.end(entity);
            }
        }
    }

    /// Lands the tick's areas, in the order of their source's stable id and then the order queued,
    /// so each takes the same id in every run: a unit of its type, of its source's team and player,
    /// with its type's tags, which triggers its delay after this tick and one tick at least, and
    /// ends at its trigger or after its duration, whichever is later. An area whose source is gone
    /// lands nothing.
    pub(super) fn land(
        mut spawner: DeliverySpawner<'_, '_>,
        mut launches: ResMut<'_, AreaLaunches>,
        (specs, tick): (Res<'_, ByType<AreaSpec>>, Res<'_, SimTick>),
    ) {
        let now = tick.start();
        launches.0.sort_by_key(|launch| launch.by.source);
        for &AreaLaunch {
            id,
            by,
            pos: at,
            unit_type,
            aimed,
        } in &launches.0
        {
            let spec = specs.get(unit_type).expect("an area's type has a spec");
            let triggers_at = now.after(spec.delay.max(Ticks::ONE));
            let ends_at = triggers_at.max(now.after(spec.duration));
            spawner.spawn(by.source, at, unit_type, id, |_| {
                (Area::new(by, aimed, ends_at), AreaTrigger::at(triggers_at))
            });
        }
        launches.0.clear();
    }
}
