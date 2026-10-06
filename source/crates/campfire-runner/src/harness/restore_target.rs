use bevy_ecs::world::World;
use campfire_common::{SegmentSeed, StateHash};
use campfire_log::internals::LogCheck;
use campfire_package::ModePackages;
use campfire_sim::{EntityIndex, SimUpdate, SnapshotError, StateRegistry, TickRate};

use crate::match_build::MatchBuild;

/// A match of a mode with its books in place and no unit, to restore snapshots into, as a session
/// that loads a save will: the books come from the packages, the state from the snapshot.
#[derive(Debug)]
pub struct RestoreTarget {
    world: World,
    registry: StateRegistry,
    /// Last, so it drops after the world and sees what the world logs as it drops.
    _log: LogCheck,
}

impl RestoreTarget {
    /// The match of `packages` for `players` players at the mode's default rate.
    pub fn new(packages: &ModePackages, players: u32) -> RestoreTarget {
        let log = LogCheck::start();
        let mut world = World::new();
        let rate = TickRate::new(packages.manifest().tick_hz.default());
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        MatchBuild::run(packages, &mut world, &mut schedule, &mut registry, players);
        world.add_schedule(schedule);
        RestoreTarget {
            world,
            registry,
            _log: log,
        }
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_sim::SimUpdate;
    use campfire_sim::internals::Draws;

    use crate::harness::restore_target::RestoreTarget;

    impl RestoreTarget {
        /// Builds the match's schedule again with no automatic sync points, as
        /// `SimUpdate::build_without_sync_points` does.
        pub fn build_without_sync_points(&mut self) -> Result<(), String> {
            SimUpdate::build_without_sync_points(&mut self.world)
        }

        /// The systems of the match's schedule in no stage and no edge set, as
        /// `SimUpdate::systems_outside_stages` names them.
        pub fn systems_outside_stages(&mut self) -> Vec<String> {
            SimUpdate::systems_outside_stages(&mut self.world)
        }

        /// The name of each state type the match registers.
        pub fn state_names(&self) -> Vec<&'static str> {
            self.registry.names().collect()
        }

        /// Puts a drawn value of the state type `name` in place of one holder's in what it
        /// holds; false when nothing holds one, or no draw decodes.
        pub fn scramble(&mut self, name: &str, draws: &mut Draws) -> bool {
            self.registry.scramble(&mut self.world, name, draws)
        }
    }
}
