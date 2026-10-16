use bevy_ecs::bundle::Bundle;
use bevy_ecs::world::World;
use campfire_actions::Actions;
use campfire_capabilities::{Match, Order};
use campfire_common::PlayerSlot;
use campfire_math::Vec3;
use campfire_mode::Mode;
use campfire_package::ModePackages;
use campfire_script::{ScriptFailure, ScriptFailures};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{SimWorld, StableId, TickRate};
use campfire_stats::{PoolId, Stats};
use campfire_test_packages::PackageTree;
use campfire_values::{ActionId, DeclaredName, ModifierId, Names, PackageIndex, Team};
use campfire_vision::Vision;

use campfire_runner::MatchBuild;

/// An arena: the books of a mode, on its only map, with every book in place but the map's vision
/// grid, which `load_vision` gives, and no mode installed. Its script never runs, so no `calc_damage` weighs a hit and
/// no map unit spawns, and its match starts with the match's entity alone. A test spawns its units
/// as the mode spawns them, each of a type the test adds to the mode, and drives their actions, as
/// the packages hold them.
#[derive(Debug)]
pub struct Arena {
    packages: ModePackages,
}

impl Arena {
    /// The arena of `tree`'s mode, the unit types of the units a test spawns and their actions
    /// added to its files.
    pub fn load(tree: &PackageTree) -> Self {
        Self {
            packages: tree.packages(),
        }
    }

    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }

    /// The arena's match at `rate`, for `players` players, which fails its test on any warning it
    /// logs, and keeps every script failure since it began.
    pub fn start(&self, rate: TickRate, players: u32) -> Match {
        let mut game = MatchBuild::arena(&self.packages, rate, players);
        game.insert_non_send(ArenaFailures::default());
        game
    }

    /// A unit of the mode's type `unit_type` and `team` at `pos`, with `parts`, spawned as the
    /// mode spawns one: with the parts its type's books give and its passive.
    pub fn spawn(
        &self,
        game: &mut World,
        unit_type: &str,
        pos: Vec3,
        team: Team,
        parts: impl Bundle,
    ) -> StableId {
        Mode::spawn_typed(game, unit_type, pos, team, parts)
    }

    /// The action `name` of the arena's package `package`.
    pub fn action(&self, game: &World, package: &str, name: &str) -> ActionId {
        Actions::named(game, self.package(package), name)
            .unwrap_or_else(|| panic!("no action {name} in {package}"))
    }

    /// The modifier `name` of the arena's package `package`.
    pub fn modifier(&self, game: &World, package: &str, name: &str) -> ModifierId {
        Stats::modifier(game, self.package(package), name)
            .unwrap_or_else(|| panic!("no modifier {name} in {package}"))
    }

    /// The pool `name` of the arena's mode.
    pub fn pool(&self, name: &str) -> PoolId {
        let pools = self.packages.data().pools.keys();
        let pools: Names<PoolId> = pools.map(DeclaredName::as_str).collect();
        pools
            .named(name)
            .unwrap_or_else(|| panic!("no pool {name}"))
    }

    /// Gives `game`, the arena's match, the map's vision grid and its brush, for `teams` teams,
    /// as the mode's install does: from the next Vision stage, each team sees what its units and
    /// its reveals show it.
    pub fn load_vision(&self, game: &mut World, teams: usize) {
        let grid = self.packages.map().grid().unwrap();
        let grid = grid.expect("the map has a vision grid");
        let brush = self.packages.map().brush().unwrap();
        Vision::load_grid(game, grid, &brush, teams);
    }

    /// Runs one tick of `game`, the arena's match, as `SimWorld::step` does, and keeps its
    /// script failures.
    pub fn step(&self, game: &mut World) {
        game.step();
        let mut kept = game
            .remove_non_send::<ArenaFailures>()
            .expect("an arena keeps its failures");
        kept.0.extend(game.non_send_mut::<ScriptFailures>().drain());
        game.insert_non_send(kept);
    }

    /// Runs a tick of `game`, the arena's match, in which player `slot` orders each of `orders`.
    pub fn tick(&self, game: &mut World, slot: u32, orders: &[Order]) {
        let payload = Order::payload(orders);
        game.push_input(PlayerSlot::new(slot), &payload);
        self.step(game);
    }

    /// Every script failure since `game`, the arena's match, began, in order.
    pub fn failures<'w>(&self, game: &'w World) -> &'w [ScriptFailure] {
        &game.non_send::<ArenaFailures>().0
    }

    /// The place among the arena's packages of the package `name`.
    fn package(&self, name: &str) -> PackageIndex {
        let at = self
            .packages
            .packages()
            .position(|view| view.package.header.name == name)
            .unwrap_or_else(|| panic!("no package {name}"));
        PackageIndex::new(u16::try_from(at).unwrap())
    }
}

/// Every script failure since an arena's match began; `ScriptFailures` keeps only the last tick's.
#[derive(Debug, Default)]
struct ArenaFailures(Vec<ScriptFailure>);
