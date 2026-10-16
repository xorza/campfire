//! What the check pins: the counts and measures the known version and this importer give.

use campfire_common::PackageFingerprint;
use campfire_import::{DropCause, WeaponDrop};
use campfire_mode::MatchResult;
use derive_more::Display;

/// The map the check plays.
pub(crate) const MAP: &str = "tournament_desert";

/// The fingerprints of the imported package and of the rules package that the known version and
/// this importer give on every OS: those a run on Linux and a run on macOS gave  (12-zero-hour.md,
/// One reproducible package; 12-zero-hour.md, Fixtures in the tests, the install on request). An
/// importer that converts anything otherwise moves them, and a run on each OS pins them again.
pub(crate) const FINGERPRINTS: Fingerprints = Fingerprints {
    game: PackageFingerprint::of_hex(
        "31fff10fc65a5636f1c1efd86f473632f1d6d016790b704cff1eda4088598093",
    ),
    rules: PackageFingerprint::of_hex(
        "d0245c9b5ddd9cb2cb58aeb2e69e92f3779b30cbbca0aa291a47ef35cd3aa874",
    ),
};

/// Tournament Desert's walls: its 48,689 cliff cells grown by one ring, merged row by row
/// (12-zero-hour.md, Cliffs are walls).
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

/// The trains and builds the import converts from every command set, and the buttons it drops, by
/// cause in the order of `DropCause::ALL`: no object, a science 22, an either-or 4, no walker 36,
/// as the aircraft an airfield or a supply center trains, no box 24, no queue, no type, a bad time
/// (12-zero-hour.md, Every command set's trains and builds are imported).
pub(crate) const PRODUCED: ProducedCounts = ProducedCounts {
    trains: 187,
    builds: 138,
    dropped: [0, 22, 4, 36, 24, 0, 0, 0],
};

/// The production play's ticks and money on Tournament Desert: the dozer's 5 s, 150 ticks, the
/// order's tick among them; the barracks' 706, the dozer's drive round the command center's box
/// to the site, which the check pins as it measured it, then its 10 s of building, 300 ticks;
/// the ranger's 5 s; and the money of the 50,000 the host picks less the dozer's 1,000, the
/// barracks' 600 and the ranger's 225.
pub(crate) const PRODUCTION: ProductionSteps = ProductionSteps {
    dozer_ticks: 150,
    barracks_ticks: 706,
    ranger_ticks: 150,
    money: [49_000, 48_400, 48_175],
};

/// The factory's chain on Tournament Desert: the dozer's walk to each site and its building, the
/// power plant's 10 s, 300 ticks, the supply center's 10 s and the war factory's 15 s, 450,
/// which the check pins as it measured them; and the money of 48,175 less the plant's 800, the
/// center's 2,000 and the factory's 2,000.
pub(crate) const FACTORY: FactorySteps = FactorySteps {
    ticks: [483, 688, 1091],
    money: 48_175 - 800 - 2000 - 2000,
};

/// The fight on Tournament Desert against the builder bot: the factory's 3 crusaders, each its
/// 10 s, 300 ticks, one after another, 900; their drive into the 150 m of their guns and the turn
/// of their turrets to the first shell's launch; its flight, on its curve's points at 400 m a
/// second, to the center's first damage; and the center's destruction, 5,000 life at 60
/// armor-piercing damage a shell, which its armor takes whole, so 84 shells of 3 guns that each
/// fire every 2 s; each of which the check pins as it measured it; the money of 48,175 less the
/// plant's, the center's and the factory's 4,800 and 3 crusaders' 900 each; and no end of the
/// match, as the bot's other buildings count for its victory (12-zero-hour.md, Defeat by the last
/// building that counts).
pub(crate) const FIGHT: FightSteps = FightSteps {
    train_ticks: 3 * 300,
    shot_ticks: Some(816),
    hit_ticks: Some(827),
    fight_ticks: 2567,
    money: 50_000 - 1825 - 4800 - 3 * 900,
    result: None,
};

/// The unit types of the bot's base, which the play counts as player 1's attack starts.
pub(crate) const BOT_TYPES: [&str; 10] = [
    "americacommandcenter",
    "americavehicledozer",
    "americapowerplant",
    "americabarracks",
    "americasupplycenter",
    "americawarfactory",
    "americainfantryranger",
    "americainfantrymissiledefender",
    "americatankcrusader",
    "americavehiclehumvee",
];

/// The builder bot's base on Tournament Desert as player 1's attack starts: its center and its
/// two dozers, each building its orders name, the barracks' two rangers and missile defender,
/// and the factory's two crusaders and humvee, of each `BOT_TYPES`' count, the last whole in tick
/// 3,850 of the play; and the farthest any of its units stood from its center, in whole meters,
/// 310, which the check pins as it measured it: its farthest site, the war factory's, √(150² +
/// 230²) = 274.6 m away, and the factory's rally beyond it (12-zero-hour.md, A bot that
/// builds and does not fight).
pub(crate) const BOT: BotBase = BotBase {
    built: [1, 2, 1, 1, 1, 1, 2, 1, 2, 1],
    farthest_m: 310,
};

/// The weapons the import converts from every starting weapon set, 29, the weapons it drops by
/// cause in the order of `WeaponDrop::ALL`: no weapon, a missile 52, a radius 9, a scatter, a
/// radius that spares its own kind 2, a shell that detonates on what is no unit, no hit on the
/// ground 7, a suicide 1, damage at its own place, a fire that speeds up 4, a delay before the
/// attack 6, a minimum range 2, a random delay, no delay 3, no damage to life 4, healing, a bad
/// number 2; its armors, `Armor.ini`'s 53, and damage kinds, the game's 38; and the unit types'
/// objects of no life, 38, and of a wounded start (12-zero-hour.md, Life is the original's;
/// 12-zero-hour.md, A weapon converts when it hits one target at once;
/// 12-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and
/// slow weapons).
pub(crate) const FOUGHT: FoughtCounts = FoughtCounts {
    weapons: 29,
    dropped: [0, 52, 9, 0, 2, 0, 7, 1, 0, 4, 6, 2, 0, 3, 4, 0, 2],
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

/// The factory's steps: the ticks of each building the dozer builds, from the tick its order
/// applies to the tick it is whole, and player 1's money after the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FactorySteps {
    pub(crate) ticks: [u64; 3],
    pub(crate) money: i64,
}

/// What the bot built: the living units of each of `BOT_TYPES` it owns as player 1's attack
/// starts, and the farthest any of its units stood from its command center over the play, in
/// whole meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BotBase {
    pub(crate) built: [usize; BOT_TYPES.len()],
    pub(crate) farthest_m: i64,
}

/// The fight's steps: the ticks from the first train to the last crusader trained, and from the
/// attack's order to the first shell's launch, to the first damage to the bot's command center,
/// and to its death; player 1's money as the attack is ordered; and how the match ended by then,
/// if it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FightSteps {
    pub(crate) train_ticks: u64,
    pub(crate) shot_ticks: Option<u64>,
    pub(crate) hit_ticks: Option<u64>,
    pub(crate) fight_ticks: u64,
    pub(crate) money: i64,
    pub(crate) result: Option<MatchResult>,
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
