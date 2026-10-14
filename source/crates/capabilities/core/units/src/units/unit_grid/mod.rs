use bevy_ecs::change_detection::{CheckChangeTicks, DetectChanges, Ref, Tick};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut, SystemChangeTick, SystemParam};
use bevy_ecs::world::World;
use campfire_declare::Cache;
use campfire_geometry::{BodyGrid, Placed};
use campfire_sim::{Position, StableId, Unpredicted};

use crate::units::body::Body;
use crate::units::body_unit::BodyUnit;

/// The body units of a match, the dead and those a client only holds among them, as one
/// `BodyGrid` by entity, built again only when a unit came, went, moved or changed its body
/// since its last build: the readers after Collide, the last stage that moves a unit, share
/// one build a tick, where each sorted every unit of its own. Each reader checks what it needs
/// of a unit, life, team, tags, as it visits. Derived from the units, never state.
#[derive(Resource, Debug, Default)]
pub struct UnitGrid {
    grid: BodyGrid<Entity>,
    /// The last build, `None` before the first.
    pub built: Option<Built>,
    /// The run of the last reader that found the grid current, which reads it again as it is.
    checked: Option<Tick>,
    /// A unit lost its body, or Bevy's check aged the change ticks, since the last build: what no
    /// change tick shows.
    dirty: bool,
}

/// A build: the run it was made in, and how many units it holds, so a unit gone shows.
#[derive(Debug, Clone, Copy)]
pub struct Built {
    pub tick: Tick,
    units: usize,
}

/// The units the grid holds, and the change ticks of what it holds of them.
type GridUnits<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, StableId>,
        Ref<'static, Position>,
        Option<Ref<'static, Body>>,
    ),
    (With<BodyUnit>, Allow<Unpredicted>),
>;

/// The grid of the body units as they stand when a reader asks.
#[derive(SystemParam, Debug)]
pub struct UnitsNear<'w, 's> {
    pub grid: ResMut<'w, UnitGrid>,
    units: GridUnits<'w, 's>,
    ticks: SystemChangeTick,
}

impl Cache for UnitGrid {}

impl UnitGrid {
    /// Puts an empty grid in `world`, and the observers that mark it dirty.
    pub(crate) fn install(world: &mut World) {
        world.insert_resource(UnitGrid::default());
        world.add_observer(
            |_: On<'_, '_, Remove, Body>, mut grid: ResMut<'_, UnitGrid>| {
                grid.dirty = true;
            },
        );
        // Bevy's check clamps old change ticks and wraps none, so a build or a check older than
        // the maximum change age reads no change as newer, and a run's tick may come again.
        world.add_observer(
            |_: On<'_, '_, CheckChangeTicks>, mut grid: ResMut<'_, UnitGrid>| {
                grid.dirty = true;
                grid.checked = None;
            },
        );
    }

    /// Whether `units` differ from the last build, as a reader in the run of `this_run` sees them.
    fn stale(&self, units: &GridUnits<'_, '_>, this_run: Tick) -> bool {
        let Some(built) = self.built.filter(|_| !self.dirty) else {
            return true;
        };
        // A run's tick is the world's before it, so a change stamped with the build's tick came
        // before the build, and its reader's deferred commands stamp a later one.
        let newer = |tick: Tick| tick.is_newer_than(built.tick, this_run);
        let mut count = 0;
        for (_, id, pos, body) in units {
            let body = body.is_some_and(|body| newer(body.last_changed()));
            if body || newer(id.last_changed()) || newer(pos.last_changed()) {
                return true;
            }
            count += 1;
        }
        count != built.units
    }

    /// Builds the grid again from `units`, in the run of `this_run`.
    fn rebuild(&mut self, units: &GridUnits<'_, '_>, this_run: Tick) {
        let mut count = 0;
        self.grid
            .rebuild(units.iter().map(|(entity, id, pos, body)| {
                count += 1;
                Placed {
                    id: *id,
                    key: entity,
                    pos: *pos,
                    shape: Body::shape_of(body.as_deref()),
                }
            }));
        self.built = Some(Built {
            tick: this_run,
            units: count,
        });
        self.dirty = false;
    }
}

impl UnitsNear<'_, '_> {
    /// The grid of the body units as they stand, built again first when stale: the first call of
    /// a run checks every unit's change ticks, and a later call of the same run reads it as it is.
    pub fn current(&mut self) -> &BodyGrid<Entity> {
        let this_run = self.ticks.this_run();
        let grid = &mut *self.grid;
        if grid.checked != Some(this_run) {
            if grid.stale(&self.units, this_run) {
                grid.rebuild(&self.units, this_run);
            }
            grid.checked = Some(this_run);
        }
        &self.grid.grid
    }
}
