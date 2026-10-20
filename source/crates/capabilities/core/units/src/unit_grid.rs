use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleConfigs};
use bevy_ecs::system::{Res, ResMut, ScheduleSystem, SystemParam};
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_geometry::{BodyGrid, Indexed, Placed};
use campfire_sim::StableId;

use crate::body::Body;
use crate::unit_entity::body_unit::BodyUnit;
use crate::unit_tracker::{TrackedUnits, Tracking};

/// The body units of a match, the dead and those a client only holds among them, as one
/// `BodyGrid` by entity, built again by `refresh` only when its tracker finds them changed: the
/// readers after Collide, the last stage that moves a unit, share one build a tick, where each
/// sorted every unit of its own. Each reader checks what it needs of a unit, life, team, tags, as
/// it visits. Derived from the units, never state.
#[derive(Resource, Debug, Default)]
pub struct UnitGrid(BodyGrid<Entity, StableId>);

/// The units the grid holds: every body unit, which holds its body as a part it may lack.
#[derive(Debug)]
pub struct GridUnits;

/// What builds the grid again: the grid, and the tracker of its units.
#[derive(SystemParam, Debug)]
pub struct GridBuild<'w, 's> {
    grid: ResMut<'w, UnitGrid>,
    tracking: Tracking<'w, 's, GridUnits>,
}

/// The grid of the body units, as the refresh right before its reader built it.
#[derive(SystemParam, Debug)]
pub struct UnitsNear<'w> {
    grid: Res<'w, UnitGrid>,
}

impl TrackedUnits for GridUnits {
    type Filter = With<BodyUnit>;
}

impl Cache for UnitGrid {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }
}

impl UnitGrid {
    /// The system that builds the grid again when its tracker finds the units changed, which runs
    /// right before each reader, so the readers take it as a plain resource and run side by side
    /// (02-engine-core.md, A shared index refreshes in its own system).
    pub fn refresh(mut build: GridBuild<'_, '_>) {
        build.refresh();
    }

    /// `systems`, which read the grid, each after a refresh of it: as each reader found the grid
    /// it built when it asked, a refresh for each finds it current.
    pub fn before_reading<M>(
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> ScheduleConfigs<ScheduleSystem> {
        (Self::refresh, systems).chain()
    }
}

impl GridBuild<'_, '_> {
    /// Builds the grid again when its tracker finds the units changed, as a reader that runs
    /// within a system that changes the units needs before it reads.
    pub fn refresh(&mut self) {
        if self.tracking.is_stale() {
            let mut count = 0;
            self.grid.0.rebuild(self.tracking.units().map(|unit| {
                count += 1;
                Indexed {
                    id: unit.id,
                    key: unit.entity,
                    body: Placed::new(unit.position, Body::shape_of(unit.body)),
                }
            }));
            self.tracking.took(count);
        }
    }
}

impl UnitsNear<'_> {
    /// The grid of the body units as they stand.
    pub fn current(&self) -> &BodyGrid<Entity, StableId> {
        &self.grid.0
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::change_detection::Tick;

    use crate::unit_grid::GridBuild;

    impl GridBuild<'_, '_> {
        /// The run of the grid's last build.
        pub fn built_at(&self) -> Option<Tick> {
            self.tracking.taken_at()
        }
    }
}
