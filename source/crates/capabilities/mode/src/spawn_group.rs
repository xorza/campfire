use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_navigation::{NavigationBook, PathEnd, WalkerOnPath};
use campfire_sim::StableId;
use campfire_units::{SpawnAt, Spawner};
use campfire_values::{PathId, Team, UnitType};

/// Units of `team` a call spawns in order at the end `from` of `path`, walking it from there as
/// one group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpawnGroup {
    pub team: Team,
    pub(crate) path: PathId,
    pub(crate) from: PathEnd,
    pub(crate) units: Vec<GroupUnit>,
}

/// A unit of a spawn group: its unit type, and the id a call took for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupUnit {
    pub(crate) unit_type: UnitType,
    pub id: StableId,
}

impl SpawnGroup {
    /// Spawns the group's units in `world`, through the mode's spawner.
    pub(crate) fn spawn(&self, world: &mut World) {
        let Some(first) = self.units.first() else {
            return;
        };
        let position = world
            .resource::<NavigationBook>()
            .paths
            .waypoint(self.path, 0, self.from)
            .expect("a path has a waypoint");
        for &GroupUnit { unit_type, id } in &self.units {
            let walker = WalkerOnPath::new(self.path, self.from, first.id);
            let at = SpawnAt {
                id,
                unit_type,
                team: self.team,
                owner: None,
                position,
                angle: Num::ZERO,
            };
            Spawner::spawn(world, at, walker);
        }
    }
}
