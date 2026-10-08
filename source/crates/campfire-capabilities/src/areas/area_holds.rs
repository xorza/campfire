use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::Position;

use crate::actions::targets::{TargetKey, Targets};
use crate::areas::area::Area;
use crate::areas::area_spec::{AreaSpec, Inside};
use crate::geometry::shape::Shape;
use crate::stats::held_modifiers::{Held, HeldModifiers};
use crate::units::body_grid::BodyGrid;
use crate::units::by_type::ByType;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::relation::Relation;

/// The modifiers each area holds on the units inside it.
#[derive(Debug)]
pub(super) struct AreaHolds;

impl AreaHolds {
    /// Lists the modifiers each area holds this tick: its type's `inside` modifier for its source,
    /// for the source's other allies and for the units that may be attacked, on each living unit
    /// whose body is within its radius, whose tags block it as a target or not, from its source, by
    /// its action at its rank.
    pub(super) fn hold_inside(
        targets: Targets<'_, '_>,
        specs: Res<'_, ByType<AreaSpec>>,
        mut held: ResMut<'_, HeldModifiers>,
        areas: Query<'_, '_, (&Position, &Team, &UnitType, &Area)>,
        mut grid: Local<'_, BodyGrid<TargetKey>>,
    ) {
        let holding = |unit_type| {
            let spec = specs.get(unit_type).expect("an area's type has a spec");
            (spec.inside != Inside::default()).then_some(spec)
        };
        if !areas
            .iter()
            .any(|(_, _, &unit_type, _)| holding(unit_type).is_some())
        {
            return;
        }
        grid.rebuild(targets.placed());
        for (&pos, &team, &unit_type, area) in &areas {
            let Some(spec) = holding(unit_type) else {
                continue;
            };
            grid.visit_near(pos, spec.radius, |body| {
                let unit = Targets::unit_of(body);
                if !targets.reaches(pos, Shape::POINT, spec.radius, &unit) {
                    return;
                }
                let by = area.by();
                let modifier = match targets.relation(team, unit.team) {
                    _ if unit.id == by.source => spec.inside.caster,
                    Relation::Friendly => spec.inside.allies,
                    Relation::Hostile | Relation::Neutral => spec.inside.enemies,
                };
                if let Some(modifier) = modifier {
                    held.0.push(Held {
                        target: unit.id,
                        modifier,
                        source: Some(by.source),
                        ability: Some(by.action),
                        rank: by.rank,
                    });
                }
            });
        }
    }
}
