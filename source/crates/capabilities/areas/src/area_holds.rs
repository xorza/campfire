use bevy_ecs::system::{Query, Res, ResMut};
use campfire_actions::Targets;
use campfire_declare::ByType;
use campfire_geometry::Placed;
use campfire_sim::Position;
use campfire_stats::{Held, HeldModifiers};
use campfire_units::UnitsNear;
use campfire_values::{Relation, Team, UnitType};

use crate::area::Area;
use crate::area_spec::{AreaSpec, Inside};

/// The modifiers each area holds on the units inside it.
#[derive(Debug)]
pub struct AreaHolds;

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
        mut near: UnitsNear<'_, '_>,
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
        let grid = near.current();
        for (&pos, &team, &unit_type, area) in &areas {
            let Some(spec) = holding(unit_type) else {
                continue;
            };
            grid.visit_near(pos, spec.radius, |body| {
                let Some(unit) = targets.living_at(body.key) else {
                    return;
                };
                if !targets.reaches(Placed::point(pos), spec.radius, &unit) {
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
                        action: Some(by.action),
                        rank: by.rank,
                    });
                }
            });
        }
    }
}
