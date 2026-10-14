use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{ResMut, SystemParam};
use campfire_declare::Cache;
use campfire_geometry::{BodyGrid, Placed};

use crate::units::body::Body;
use crate::units::body_unit::BodyUnit;
use crate::units::unit_tracker::{TrackedUnits, Tracking};

/// The body units of a match, the dead and those a client only holds among them, as one
/// `BodyGrid` by entity, built again only when its tracker finds them changed: the readers after
/// Collide, the last stage that moves a unit, share one build a tick, where each sorted every unit
/// of its own. Each reader checks what it needs of a unit, life, team, tags, as it visits. Derived
/// from the units, never state.
#[derive(Resource, Debug, Default)]
pub struct UnitGrid(BodyGrid<Entity>);

/// The units the grid holds: every body unit, which holds its body as a part it may lack.
#[derive(Debug)]
pub struct GridUnits;

/// The grid of the body units as they stand when a reader asks.
#[derive(SystemParam, Debug)]
pub struct UnitsNear<'w, 's> {
    grid: ResMut<'w, UnitGrid>,
    tracking: Tracking<'w, 's, GridUnits>,
}

impl TrackedUnits for GridUnits {
    type Filter = With<BodyUnit>;
}

impl Cache for UnitGrid {}

impl UnitsNear<'_, '_> {
    /// The grid of the body units as they stand, built again first when its tracker finds them
    /// changed.
    pub fn current(&mut self) -> &BodyGrid<Entity> {
        if self.tracking.stale() {
            let mut count = 0;
            self.grid.0.rebuild(self.tracking.units().map(|unit| {
                count += 1;
                Placed {
                    id: unit.id,
                    key: unit.entity,
                    pos: unit.pos,
                    shape: Body::shape_of(unit.body),
                }
            }));
            self.tracking.took(count);
        }
        &self.grid.0
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::change_detection::Tick;

    use crate::units::unit_grid::UnitsNear;

    impl UnitsNear<'_, '_> {
        /// The run of the grid's last build.
        pub fn built_at(&self) -> Option<Tick> {
            self.tracking.taken_at()
        }
    }
}
