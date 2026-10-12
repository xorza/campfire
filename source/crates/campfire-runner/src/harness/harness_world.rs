use bevy_ecs::world::World;
use campfire_capabilities::internals::{
    ActionsWorldInternals, Density, KernelScene, ModeWorldInternals, StatsWorldInternals,
};
use campfire_capabilities::{
    ActionId, ModifierId, Order, PoolId, ScriptFailure, ScriptFailures, Team, VisionWorld,
};
use campfire_common::PlayerSlot;
use campfire_package::ModePackages;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{EntityIndex, Position, SimWorld, SnapshotError};

use crate::harness::match_units::MatchUnits;

/// What the harness gives a match for the tests and benches of every crate: the units by role,
/// an arena's packages and failures, and a restore target's restores.
pub trait HarnessWorldInternals {
    /// The units of the match, as a scripted player or a test names them by role.
    fn match_units(&self) -> MatchUnits<'_>;

    /// Gives an arena the map's vision grid and its brush, for `teams` teams, as the mode's
    /// install does: from the next Vision stage, each team sees what its units and its reveals
    /// show it.
    fn load_arena_vision(&mut self, teams: usize);

    /// The action `name` of an arena's package `package`.
    fn arena_action(&self, package: &str, name: &str) -> ActionId;

    /// The modifier `name` of an arena's package `package`.
    fn arena_modifier(&self, package: &str, name: &str) -> ModifierId;

    /// The pool `name` of an arena's mode.
    fn arena_pool(&self, name: &str) -> PoolId;

    /// Runs one tick of an arena, as `SimWorld::step` does, and keeps its script failures.
    fn arena_step(&mut self);

    /// Runs a tick of an arena in which player `slot` orders each of `orders`.
    fn arena_tick(&mut self, slot: u32, orders: &[Order]);

    /// Every script failure since an arena's match began, in order.
    fn arena_failures(&self) -> &[ScriptFailure];

    /// Restores `snapshot` into a restore target, in place of what the last restore left.
    fn restore_in_place(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError>;

    /// Spawns `KernelScene::UNITS` of the 3v3's creeps, on both teams in turn by pairs, at the
    /// crowded scene's points, which lie within its map.
    fn crowd_creeps(&mut self);
}

impl HarnessWorldInternals for World {
    fn match_units(&self) -> MatchUnits<'_> {
        MatchUnits { world: self }
    }

    fn load_arena_vision(&mut self, teams: usize) {
        let packages = &self.non_send::<ArenaPackages>().0;
        let grid = packages.map().grid().unwrap();
        let grid = grid.expect("the map has a vision grid");
        let brush = packages.map().brush().unwrap();
        self.load_vision_grid(grid, &brush, teams);
    }

    fn arena_action(&self, package: &str, name: &str) -> ActionId {
        self.action_named(arena_package(self, package), name)
            .unwrap_or_else(|| panic!("no action {name} in {package}"))
    }

    fn arena_modifier(&self, package: &str, name: &str) -> ModifierId {
        self.modifier_named(arena_package(self, package), name)
            .unwrap_or_else(|| panic!("no modifier {name} in {package}"))
    }

    fn arena_pool(&self, name: &str) -> PoolId {
        let packages = &self.non_send::<ArenaPackages>().0;
        PoolId::named(&packages.data().pools, name).unwrap_or_else(|| panic!("no pool {name}"))
    }

    fn arena_step(&mut self) {
        self.step();
        let mut kept = self
            .remove_non_send::<ArenaFailures>()
            .expect("an arena keeps its failures");
        kept.0.extend(self.non_send_mut::<ScriptFailures>().drain());
        self.insert_non_send(kept);
    }

    fn arena_tick(&mut self, slot: u32, orders: &[Order]) {
        let payload = Order::payload(orders);
        self.push_input(PlayerSlot::new(slot), &payload);
        self.arena_step();
    }

    fn arena_failures(&self) -> &[ScriptFailure] {
        &self.non_send::<ArenaFailures>().0
    }

    fn restore_in_place(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError> {
        let units: Vec<_> = self
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| entity)
            .collect();
        for entity in units {
            self.despawn(entity);
        }
        self.restore(snapshot)
    }

    fn crowd_creeps(&mut self) {
        let mut scene = KernelScene::new(13);
        for unit in 0..KernelScene::UNITS {
            let pos = Position::new(scene.point(Density::Crowded.span())).unwrap();
            let team = Team::new(u8::from(unit % 4 >= 2));
            self.spawn_typed(CREEPS[unit % 2], team, pos);
        }
    }
}

/// The creep types of the 3v3's waves, which a crowd's units take in turn.
const CREEPS: [&str; 2] = ["melee_creep", "caster_creep"];

/// The place among an arena's packages of the package `name`.
fn arena_package(world: &World, name: &str) -> u16 {
    let at = world
        .non_send::<ArenaPackages>()
        .0
        .packages()
        .position(|view| view.package.header.name == name)
        .unwrap_or_else(|| panic!("no package {name}"));
    u16::try_from(at).unwrap()
}

/// The packages an arena's match holds the books of.
#[derive(Debug)]
pub(crate) struct ArenaPackages(pub(crate) ModePackages);

/// Every script failure since an arena's match began; `ScriptFailures` keeps only the last tick's.
#[derive(Debug, Default)]
pub(crate) struct ArenaFailures(pub(crate) Vec<ScriptFailure>);
