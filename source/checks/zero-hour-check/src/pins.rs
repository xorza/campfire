//! What the check pins: the counts and measures the known version and this importer give.

use campfire_common::PackageFingerprint;
use campfire_import::{DropCause, WeaponDrop};
use campfire_mode::MatchResult;
use campfire_values::Team;
use derive_more::Display;

/// The map the check plays.
pub(crate) const MAP: &str = "tournament_desert";

/// The fingerprints of the imported package and of the rules package that the known version and
/// this importer give on every OS: those a run on Linux and a run on macOS gave (design 12, D4,
/// D7). An importer that converts anything otherwise moves them, and a run on each OS pins them
/// again.
pub(crate) const FINGERPRINTS: Fingerprints = Fingerprints {
    game: PackageFingerprint::of_hex(
        "66b625667c81ebd360d2aa3e7318d86c1512d6fc158296e621b4d445ebc30c23",
    ),
    rules: PackageFingerprint::of_hex(
        "d0245c9b5ddd9cb2cb58aeb2e69e92f3779b30cbbca0aa291a47ef35cd3aa874",
    ),
};

/// Tournament Desert's walls: its 48,689 cliff cells grown by one ring, merged row by row
/// (design 17, D3).
pub(crate) const WALLS: usize = 286;

/// The most ticks player 1's dozer takes from its start to player 2's on Tournament Desert, at 30
/// ticks a second, which the check pins: what it took, 57.2 s.
pub(crate) const DRIVE_TICKS: u64 = 1717;

/// The looks the import gives night or snow variants, of unit types and of client objects; the
/// unit types' looks of no models, of objects that draw nothing; and the maps of night and of
/// snow among the 150.
pub(crate) const LOOKS: LookCounts = LookCounts {
    varied: 382,
    empty: 273,
    night: 15,
    snowy: 17,
};

/// The trains and builds the import converts from every command set, and the buttons it drops,
/// by cause in the order of `DropCause::ALL`: no object, a science 22, an either-or 4, no walker 36,
/// as the aircraft an airfield or a supply center trains, no box 24, no queue, no type, a bad
/// time (design 18, D3).
pub(crate) const PRODUCED: ProducedCounts = ProducedCounts {
    trains: 187,
    builds: 138,
    dropped: [0, 22, 4, 36, 24, 0, 0, 0],
};

/// The production play's ticks and money on Tournament Desert: the dozer's 5 s, 150 ticks, the
/// order's tick among them; the barracks' 706, the dozer's drive round the command center's box
/// to the site, which the check pins as it measured it, then its 10 s of building, 300 ticks;
/// the ranger's 5 s; and the money of 10,000 less the dozer's 1,000, the barracks' 600 and the
/// ranger's 225.
pub(crate) const PRODUCTION: ProductionSteps = ProductionSteps {
    dozer_ticks: 150,
    barracks_ticks: 706,
    ranger_ticks: 150,
    money: [9000, 8400, 8175],
};

/// The fight on Tournament Desert against an idle bot: the barracks' 19 more rangers, the first
/// of the production steps trained, each its 150 ticks one after another, 2,850; their walk to
/// the bot's command center and its destruction, 5,000 life at a ranger's 5 small arms halved,
/// which the check pins as it measured it; the money of 8,175 less 19 rangers' 225 each; player
/// 1's team the winner; and the bot's dozer's life gone.
pub(crate) const FIGHT: FightSteps = FightSteps {
    train_ticks: 19 * 150,
    fight_ticks: 3199,
    money: 8175 - 19 * 225,
    result: MatchResult::Won(Team::new(0)),
    killed: true,
};

/// The weapons the import converts from every starting weapon set, the weapons it drops by cause
/// in the order of `WeaponDrop::ALL`: no weapon, a projectile 70, a radius 6, a scatter, a speed
/// that may delay the damage 7, no hit on the ground 7, a suicide, damage at its own place, a
/// fire that speeds up 3, a delay before the attack 4, a minimum range, a random delay, no delay
/// 3, no damage to life 4, healing, a bad number; its armors, `Armor.ini`'s 53,
/// and damage kinds, the game's 38; and the unit types' objects of no life, 38, and of a wounded
/// start (design 19, D5, D6).
pub(crate) const FOUGHT: FoughtCounts = FoughtCounts {
    weapons: 17,
    dropped: [0, 70, 6, 0, 7, 7, 0, 0, 3, 4, 0, 0, 3, 4, 0, 0],
    armors: 53,
    kinds: 38,
    unliving: 38,
    wounded: 0,
};

/// Tournament Desert's terrain: a ground quad for each of the 269 × 339 cells the game draws;
/// its overlays; a copy for each tile its classes' textures load; and a border wrap for each
/// class's block.
pub(crate) const TERRAIN: TerrainDrawn = TerrainDrawn {
    quads: 269 * 339,
    overlays: 19_041,
    copies: 300,
    wraps: 19,
};

/// Tournament Desert's height samples, as columns and rows.
pub(crate) const HEIGHTS: [u32; 2] = [270, 340];

/// Tournament Desert's placed units, of its 580 objects once its 176 road ends are dropped, and
/// their types.
pub(crate) const UNITS: usize = 142;
pub(crate) const UNIT_TYPES: usize = 41;

/// Tournament Desert's client objects, the rest of its objects, and their looks.
pub(crate) const CLIENT_OBJECTS: usize = 438;
pub(crate) const OBJECT_LOOKS: usize = 22;

/// Tournament Desert's markers.
pub(crate) const MARKERS: usize = 28;

/// How many looks have night or snow variants, how many have no models, and how many maps are
/// night maps and snowy maps.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display("{varied} varied looks, {empty} empty looks, {night} night maps and {snowy} snowy maps")]
pub(crate) struct LookCounts {
    pub(crate) varied: usize,
    pub(crate) empty: usize,
    pub(crate) night: usize,
    pub(crate) snowy: usize,
}

/// The production play's steps: each one's ticks, from the tick its order applies to the tick
/// its unit or building is whole, and player 1's money after each pays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProductionSteps {
    pub(crate) dozer_ticks: u64,
    pub(crate) barracks_ticks: u64,
    pub(crate) ranger_ticks: u64,
    pub(crate) money: [i64; 3],
}

/// The fight's steps: the ticks from the first further train to the last ranger trained, and from
/// the attack's order to the match's end; player 1's money as the attack is ordered; how the
/// match ended; and whether the bot's dozer's life is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FightSteps {
    pub(crate) train_ticks: u64,
    pub(crate) fight_ticks: u64,
    pub(crate) money: i64,
    pub(crate) result: MatchResult,
    pub(crate) killed: bool,
}

/// How many weapons the import converts, how many it drops for each cause, in the order of the
/// causes, how many armors and damage kinds it writes, and how many unit types' objects it gives
/// no life, or a life that starts below its most.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display(
    "{weapons} weapons, dropped {:?}, {armors} armors, {kinds} damage kinds, {unliving} unliving, {wounded} wounded",
    dropped
)]
pub(crate) struct FoughtCounts {
    pub(crate) weapons: usize,
    pub(crate) dropped: [usize; WeaponDrop::ALL.len()],
    pub(crate) armors: usize,
    pub(crate) kinds: usize,
    pub(crate) unliving: usize,
    pub(crate) wounded: usize,
}

/// How many trains and builds the import converts, and how many buttons it drops for each
/// cause, in the order of the causes.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display("{trains} trains, {builds} builds, dropped {:?}", dropped)]
pub(crate) struct ProducedCounts {
    pub(crate) trains: usize,
    pub(crate) builds: usize,
    pub(crate) dropped: [usize; DropCause::ALL.len()],
}

/// How many ground quads, overlays, texture copies and border wraps a terrain file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainDrawn {
    pub(crate) quads: usize,
    pub(crate) overlays: usize,
    pub(crate) copies: usize,
    pub(crate) wraps: usize,
}

/// The fingerprints of an import: the imported package's and the rules package's.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display("the fingerprints {game} and rules {rules}")]
pub(crate) struct Fingerprints {
    pub(crate) game: PackageFingerprint,
    pub(crate) rules: PackageFingerprint,
}
