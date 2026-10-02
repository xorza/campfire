use bevy_ecs::world::World;
use campfire_common::SegmentSeed;
use campfire_package::ModePackages;
use campfire_sim::{EntityIndex, SimUpdate, SnapshotError, StateHash, StateRegistry, TickRate};

use crate::match_build::MatchBuild;

/// A match of a mode with its books in place and no unit, to restore snapshots into, as a session
/// that loads a save will: the books come from the packages, the state from the snapshot.
#[derive(Debug)]
pub struct RestoreTarget {
    world: World,
    registry: StateRegistry,
}

impl RestoreTarget {
    /// The match of `packages` for `players` players at the mode's default rate.
    pub fn new(packages: &ModePackages, players: u32) -> RestoreTarget {
        let mut world = World::new();
        let rate = TickRate::new(packages.manifest().tick_hz.default());
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        MatchBuild::run(packages, &mut world, &mut schedule, &mut registry, players);
        world.add_schedule(schedule);
        RestoreTarget { world, registry }
    }

    /// Writes the snapshot of the match in `world`, of the same mode, into `out`.
    pub fn snapshot(&self, world: &World, out: &mut Vec<u8>) {
        self.registry.snapshot(world, out);
    }

    /// Restores `snapshot` in place of what the last restore left.
    pub fn restore(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError> {
        let units: Vec<_> = self
            .world
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| entity)
            .collect();
        for entity in units {
            self.world.despawn(entity);
        }
        self.registry.restore(snapshot, &mut self.world)
    }

    /// Writes the snapshot of what it holds into `out`.
    pub fn snapshot_own(&self, out: &mut Vec<u8>) {
        self.registry.snapshot(&self.world, out);
    }

    /// Runs a tick of what it holds, with no inputs.
    pub fn run_tick(&mut self) {
        self.world.run_schedule(SimUpdate);
    }

    /// The state hash of what it holds.
    pub fn hash(&self) -> StateHash {
        self.registry.hash(&self.world)
    }
}
