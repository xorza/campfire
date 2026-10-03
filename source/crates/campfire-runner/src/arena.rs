use std::path::Path;

use bevy_ecs::world::World;
use campfire_capabilities::{
    ActionId, Actions, DeclaredName, ModeInputs, ModifierId, Order, PoolId, ScriptFailure,
    ScriptFailures, Stats, Vision,
};
use campfire_common::{PlayerSlot, SegmentSeed};
use campfire_log::internals::LogCheck;
use campfire_package::ModePackages;
use campfire_sim::{SimUpdate, StateRegistry, TickInput, TickInputs, TickRate};

use crate::match_build::MatchBuild;

/// A match of a mode's packages with every book in place, the mode's own among them, and no mode
/// installed: its script never runs, so no `calc_damage` weighs a hit and no map unit spawns. A
/// test places its units by hand and drives their actions, as the packages hold them.
#[derive(Debug)]
pub struct Arena {
    world: World,
    packages: ModePackages,
    /// Every script failure since the match began; `ScriptFailures` keeps only the last tick's.
    failed: Vec<ScriptFailure>,
    /// Last, so it drops after the world and sees what the world logs as it drops.
    _log: LogCheck,
}

impl Arena {
    /// The arena of the mode at `dir` at `rate`, for `players` players.
    pub fn new(dir: &Path, rate: TickRate, players: u32) -> Arena {
        let log = LogCheck::start();
        let packages = ModePackages::from_dir(dir).unwrap_or_else(|error| panic!("{error}"));
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        let ModeInputs { books, .. } =
            MatchBuild::books(&packages, &mut world, &mut schedule, &mut registry, players);
        books.install(&mut world);
        world.add_schedule(schedule);
        Arena {
            world,
            packages,
            failed: Vec::new(),
            _log: log,
        }
    }

    pub const fn world(&self) -> &World {
        &self.world
    }

    pub const fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Gives the match the map's vision grid, for `teams` teams, as the mode's install does: from
    /// the next Vision stage, each team sees what its units and its reveals show it.
    pub fn load_vision(&mut self, teams: usize) {
        let grid = self
            .packages
            .map()
            .grid()
            .unwrap_or_else(|error| panic!("{error}"));
        let grid = grid.expect("the map has a vision grid");
        Vision::load_grid(&mut self.world, grid, teams);
    }

    /// The place among the match's packages of the package `name`.
    fn package(&self, name: &str) -> u16 {
        let at = self
            .packages
            .packages()
            .position(|view| view.package.header.name == name)
            .unwrap_or_else(|| panic!("no package {name}"));
        u16::try_from(at).unwrap()
    }

    /// The action `name` of the package `package`.
    pub fn action(&self, package: &str, name: &str) -> ActionId {
        Actions::action(&self.world, self.package(package), name)
            .unwrap_or_else(|| panic!("no action {name} in {package}"))
    }

    /// The modifier `name` of the package `package`.
    pub fn modifier(&self, package: &str, name: &str) -> ModifierId {
        Stats::modifier(&self.world, self.package(package), name)
            .unwrap_or_else(|| panic!("no modifier {name} in {package}"))
    }

    /// The pool `name` of the mode.
    pub fn pool(&self, name: &str) -> PoolId {
        let name = DeclaredName::new(name).unwrap_or_else(|| panic!("{name} is no name"));
        PoolId::named(&self.packages.data().pools, &name)
            .unwrap_or_else(|| panic!("no pool {name}"))
    }

    /// Runs one tick, and keeps its script failures.
    pub fn step(&mut self) {
        self.world.run_schedule(SimUpdate);
        let failures = self.world.non_send::<ScriptFailures>().get();
        self.failed.extend_from_slice(failures);
    }

    /// Runs a tick in which player `slot` orders each of `orders`.
    pub fn tick(&mut self, slot: u32, orders: &[Order]) {
        let payload = Order::payload(orders);
        self.world.resource_mut::<TickInputs>().push(TickInput {
            slot: PlayerSlot::new(slot),
            payload: &payload,
        });
        self.step();
    }

    /// Every script failure since the match began, in order.
    pub fn failures(&self) -> &[ScriptFailure] {
        &self.failed
    }
}
