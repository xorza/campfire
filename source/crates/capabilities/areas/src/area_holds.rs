use bevy_ecs::system::{Query, Res, ResMut};
use campfire_declarations::ById;
use campfire_sim::Position;
use campfire_stats::{HeldModifiers, Projection};
use campfire_values::{Team, UnitType};

use crate::area::Area;
use crate::area_spec::AreaSpec;

/// The modifiers each area holds on the units inside it.
#[derive(Debug)]
pub struct AreaHolds;

impl AreaHolds {
    /// Projects the modifiers each area holds this tick: its type's `inside` modifier for its
    /// source, for the source's other allies and for the other units that may be attacked, on the
    /// units within its radius, from its source, by its action at its rank. Stats select them as
    /// they select an aura's (04-capabilities/stats.md, An area holds as an aura does).
    pub(super) fn hold_inside(
        specs: Res<'_, ById<UnitType, AreaSpec>>,
        mut held: ResMut<'_, HeldModifiers>,
        areas: Query<'_, '_, (&Position, &Team, &UnitType, &Area)>,
    ) {
        for (&position, &team, &unit_type, area) in &areas {
            let spec = specs.get(unit_type).expect("an area's type has a spec");
            let by = area.by();
            for (affects, modifier) in spec.inside.holds() {
                held.project(Projection {
                    at: position.get(),
                    radius: spec.radius,
                    team,
                    affects,
                    modifier,
                    source: by.source,
                    action: Some(by.action),
                    rank: by.rank,
                });
            }
        }
    }
}
