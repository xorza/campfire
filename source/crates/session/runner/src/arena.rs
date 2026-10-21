use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use bevy_ecs::world::World;
use campfire_actions::Actions;
use campfire_capabilities::{Match, MatchKind, Order, Release};
use campfire_common::{DeclaredName, Seat, SegmentSeed};
use campfire_log::internals::LogCheck;
use campfire_math::Vec3;
use campfire_package::ModePackages;
use campfire_players::Players;
use campfire_script::{ScriptFailure, ScriptFailures};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{MatchRate, SimWorld, StableId};
use campfire_stats::{ModePoolsData, Stats};
use campfire_units::internals::Spawn;
use campfire_units::{MapBook, PoolId};
use campfire_values::{ActionId, ModifierId, Names, PackageIndex, Team};
use campfire_vision::VisionBook;

use crate::match_build::MatchBuild;

/// An arena: the books of a mode, on its only map, with every book in place but the map's vision
/// grid, which only `start_seeing` gives, and no mode installed. Its script never runs, so no
/// `calc_damage` weighs a hit and no map unit spawns, and its match starts with the match's entity
/// alone. A test spawns its units as the mode spawns them, each of a type the test adds to the
/// mode, and drives their actions, as the packages hold them.
#[derive(Debug)]
pub struct Arena {
    packages: ModePackages,
}

impl Arena {
    /// The arena of `packages`' mode, whose files a test gave the unit types of the units it
    /// spawns and their actions.
    pub const fn of(packages: ModePackages) -> Self {
        Self { packages }
    }

    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }

    /// The arena's match at `rate`, for `players` players, which fails its test on any warning it
    /// logs, and keeps every script failure since it began.
    pub fn start(&self, rate: MatchRate, players: u32) -> Match {
        self.start_with(rate, players, VisionBook(None))
    }

    /// The arena's match `start` gives, with the map's vision grid and its brush, for `teams`
    /// teams, as the mode's books build them: from its first Vision stage, each team sees what
    /// its units and its reveals show it.
    pub fn start_seeing(&self, rate: MatchRate, players: u32, teams: usize) -> Match {
        let map = self.packages.map().sections();
        let ground = MapBook::of(map, None).unwrap();
        let vision = VisionBook::of(&ground, map, teams).unwrap();
        assert!(vision.grid().is_some(), "the map has a vision grid");
        self.start_with(rate, players, vision)
    }

    /// The arena's match at `rate`, for `players` players, its map's vision `vision`: every book
    /// of the packages in place but that one, no mode installed, and the packages' scripts
    /// compiled; it starts with the match's entity alone.
    fn start_with(&self, rate: MatchRate, players: u32, vision: VisionBook) -> Match {
        let log = LogCheck::start();
        let spec = MatchBuild::spec(&self.packages, SegmentSeed::ZERO, rate, players);
        let mut books = self.packages.books(rate);
        books.replace(vision);
        let mut game = Match::new(spec, books, MatchKind::Rules).with_log(log);
        Release::compile(&mut game, &self.packages.scripts());
        Players::start_without_mode(&mut game, [], 0);
        game.insert_non_send(ArenaFailures::default());
        game
    }

    /// A unit of the mode's type `unit_type` and `team` at `position`, with `parts`, spawned as the
    /// mode spawns one: with the parts its type's books give and its passive.
    pub fn spawn(
        &self,
        game: &mut World,
        unit_type: &str,
        position: Vec3,
        team: Team,
        parts: impl Bundle<Effect: NoBundleEffect>,
    ) -> StableId {
        Spawn::named(game, unit_type, position)
            .team(team)
            .parts(parts)
            .place(game)
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
        let data = self.packages.data().section_or_default::<ModePoolsData>();
        let pools: Names<PoolId> = data.0.keys().map(DeclaredName::as_str).collect();
        pools
            .named(name)
            .unwrap_or_else(|| panic!("no pool {name}"))
    }

    /// Runs one tick of `game`, the arena's match, as `SimWorld::step` does, and keeps its
    /// script failures.
    pub fn step(&self, game: &mut World) {
        game.step();
        let mut kept = game
            .remove_non_send::<ArenaFailures>()
            .expect("an arena keeps its failures");
        kept.0.extend(game.resource_mut::<ScriptFailures>().drain());
        game.insert_non_send(kept);
    }

    /// Runs a tick of `game`, the arena's match, in which player `seat` orders each of `orders`.
    pub fn tick(&self, game: &mut World, seat: u32, orders: &[Order]) {
        let payload = Order::payload(orders);
        game.push_input(Seat::new(seat), &payload);
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
