use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_common::Ticks;
use campfire_math::Num;
use campfire_sim::{Keyed, Ordered, Position, SimTick, StableId};

use crate::actions::targets::{TargetKey, Targets};
use crate::areas::area::Area;
use crate::areas::area_launches::{AreaLaunch, AreaLaunches};
use crate::areas::area_spec::AreaSpec;
use crate::deliveries::Deliveries;
use crate::deliveries::delivered::{Delivered, Reached};
use crate::deliveries::delivery_spawner::DeliverySpawner;
use crate::geometry::shape::Shape;
use crate::units::body_grid::BodyGrid;
use crate::units::by_type::ByType;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::hit::Hit;

/// The life of the areas: each lands from its launch, then triggers and ends.
#[derive(Debug)]
pub(super) struct AreaLife;

impl AreaLife {
    /// Triggers each area whose delay ended, and ends each whose time is up, in the order of their
    /// stable ids. A trigger reaches each living unit its type's `affects` selects whose body comes
    /// within its radius, whose tags block it as a target or not, by stable id, each with its
    /// action's `on_hit`; an end runs `on_end`, and the area despawns after the hooks. Each hit is
    /// at the area's centre, of no distance or direction.
    pub(super) fn trigger(
        targets: Targets<'_, '_>,
        (specs, tick): (Res<'_, ByType<AreaSpec>>, Res<'_, SimTick>),
        mut deliveries: ResMut<'_, Deliveries>,
        mut areas: Query<'_, '_, (Entity, &StableId, &Position, &Team, &UnitType, &mut Area)>,
        (mut order, mut reached, mut grid): (
            Local<'_, Ordered>,
            Local<'_, Vec<StableId>>,
            Local<'_, BodyGrid<TargetKey>>,
        ),
    ) {
        let now = tick.start();
        let triggers = areas
            .iter()
            .any(|(.., area)| area.triggers_at().is_some_and(|at| at <= now));
        if triggers {
            grid.rebuild(targets.placed());
        }
        let placed = areas.iter().map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { id, entity } in order.sort(placed) {
            let (_, _, &pos, &team, &unit_type, mut area) =
                areas.get_mut(entity).expect("an area in the order");
            let hit = Hit {
                delivery: Some(id),
                target: area.aimed(),
                pos,
                distance: Num::ZERO,
                direction: None,
            };
            let by = area.by();
            let delivered = |reach| Delivered { by, reach, hit };
            if area.triggers_at().is_some_and(|at| at <= now) {
                let spec = specs.get(unit_type).expect("an area's type has a spec");
                reached.clear();
                grid.visit_near(pos, spec.radius, |body| {
                    let unit = Targets::unit_of(body);
                    let relation = targets.relation(team, unit.team);
                    if spec.affects.selects(relation, unit.tags)
                        && targets.reaches(pos, Shape::POINT, spec.radius, &unit)
                    {
                        reached.push(unit.id);
                    }
                });
                reached.sort_unstable();
                let hits = reached.iter().map(|&unit| delivered(Reached::Hit(unit)));
                deliveries.delivered.extend(hits);
                area.trigger();
            }
            if area.triggers_at().is_none() && area.ends_at() <= now {
                deliveries.delivered.push(delivered(Reached::End));
                deliveries.ended.push(entity);
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
            at,
            unit_type,
            aimed,
        } in &launches.0
        {
            let spec = specs.get(unit_type).expect("an area's type has a spec");
            let triggers_at = now.after(spec.delay.max(Ticks::ONE));
            let ends_at = triggers_at.max(now.after(spec.duration));
            spawner.spawn(by.source, at, unit_type, id, |_| {
                Area::new(by, aimed, Some(triggers_at), ends_at)
                    .expect("an area triggers before it ends")
            });
        }
        launches.0.clear();
    }
}
