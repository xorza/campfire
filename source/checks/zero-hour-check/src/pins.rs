//! What the check expects: what the game's data gives by a closed form, and the shapes of what
//! it measures, which `golden.toml` pins.

use campfire_common::PackageFingerprint;
use campfire_import::{ButtonDropCause, SupplyDropCause, WeaponDropCause};
use campfire_mode::MatchResult;
use derive_more::Display;
use serde::{Deserialize, Serialize};

/// The map the check plays.
pub(crate) const MAP: &str = "tournament_desert";

/// The ticks a second the plays run at.
pub(crate) const HZ: u64 = 30;

/// The money of the 50,000 the host picks less the dozer's 1,000, then the barracks' 600, then
/// the ranger's 225.
const PRODUCTION_MONEY: [i64; 3] = [50_000 - 1000, 50_000 - 1600, 50_000 - 1825];

/// The money after the production less the power plant's 800, the supply center's 2,000 and the
/// war factory's 2,000.
const FACTORY_MONEY: i64 = PRODUCTION_MONEY[2] - 800 - 2000 - 2000;

/// How many crusaders the war factory trains and sends to fight, as the money left pays for.
pub(crate) const CRUSADERS: u16 = 3;

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

/// Tournament Desert as the import writes it: its height samples, as columns and rows; its placed
/// units, once its road ends are dropped, and their types; its client objects, the rest of its
/// objects, and their looks; its markers; and its walls, its cliff cells grown by one ring and
/// merged row by row (09-zero-hour.md, Cliffs are walls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MapCounts {
    pub(crate) heights: [u32; 2],
    pub(crate) units: usize,
    pub(crate) unit_types: usize,
    pub(crate) client_objects: usize,
    pub(crate) object_looks: usize,
    pub(crate) markers: usize,
    pub(crate) walls: usize,
}

/// A terrain file's overlays, a copy for each tile its classes' textures load, and a border wrap
/// for each class's block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerrainCounts {
    pub(crate) overlays: usize,
    pub(crate) copies: usize,
    pub(crate) wraps: usize,
}

/// How many looks have night or snow variants, of unit types and of client objects; how many unit
/// types' looks have no models, of objects that draw nothing; and how many maps are night maps and
/// snowy maps.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display("{varied} varied looks, {empty} empty looks, {night} night maps and {snowy} snowy maps")]
#[serde(deny_unknown_fields)]
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
/// whole meters (09-zero-hour.md, A bot that builds and does not fight).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BotBase {
    pub(crate) built: [usize; BOT_TYPES.len()],
    pub(crate) farthest_m: i64,
}

/// How a play's match of two bots stood as it ended or its limit passed: the tick it ended and its
/// result, if it did, the buildings each player slot held, each whole and alive, and the most
/// operations one think of its bots ran, which the mode's call limit bounds (09-zero-hour.md, A
/// bots' match that ends; 09-zero-hour.md, The bots' limits are measured).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BotMatchEnd {
    pub(crate) ended: Option<u64>,
    pub(crate) result: Option<MatchResult>,
    pub(crate) standing: [usize; 2],
    pub(crate) think_ops: u64,
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

/// How many deliveries of its free truck the supply play pins.
pub(crate) const DELIVERIES: usize = 3;

/// The money the supply play's player starts with, the 50,000 its host picks, less China's power
/// plant's 1,000 and supply center's 1,500.
const SUPPLY_MONEY: i64 = 50_000 - 1000 - 1500;

/// What a delivery of the free truck pays: its 4 boxes of `ValuePerSupplyBox`, 75 in the shipped
/// `GameData.ini` (09-zero-hour.md, A box is money).
const TRUCK_LOAD: i64 = 4 * 75;

/// The supply play's steps: the ticks of the power plant and the supply center, each from its
/// order to its whole building, and the money left; and each of the free truck's first
/// `DELIVERIES` deliveries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SupplySteps {
    pub(crate) built: [u64; 2],
    pub(crate) built_money: i64,
    pub(crate) deliveries: [SupplyDelivery; DELIVERIES],
}

/// A delivery: its tick, counted from the supply center's completion, and the money after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SupplyDelivery {
    pub(crate) ticks: u64,
    pub(crate) money: i64,
}

/// The ticks of the supply play's steps no closed form gives: each building's, and each
/// delivery's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupplyTicks {
    pub(crate) built: [u64; 2],
    pub(crate) deliveries: [u64; DELIVERIES],
}

/// The ticks of the defense play's steps no closed form gives: the power plant's and the Gattling
/// Cannon's, and from the bot dozer's order to its first damage, which the cannon deals ordered
/// nothing, and to its death.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DefenseTicks {
    pub(crate) built: [u64; 2],
    pub(crate) hit: u64,
    pub(crate) death: u64,
}

/// The ticks of the missile play's steps no closed form gives: the barracks' and the Tank
/// Hunter's, and from the bot dozer's order to the first steered missile, which the hunter fires
/// ordered nothing, to the dozer's first damage, and to its death.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MissileTicks {
    pub(crate) built: u64,
    pub(crate) trained: u64,
    pub(crate) shot: u64,
    pub(crate) hit: u64,
    pub(crate) death: u64,
}

/// The ticks of the skirmish's steps no closed form gives: the barracks', from its order to its
/// whole building, its dozer's drive round the command center's box to the site among them; each
/// of the factory's chain's; and the fight's, from the attack's order to the first shell's launch,
/// to the first damage to the bot's command center, and to its death.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkirmishTicks {
    pub(crate) barracks: u64,
    pub(crate) factory: [u64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shot: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hit: Option<u64>,
    pub(crate) fight: u64,
}

/// How many weapons the import converts from every starting weapon set, how many it drops for each
/// cause, in the order of `WeaponDropCause::ALL`, how many armors and damage kinds it writes, and
/// how many unit types' objects it gives no life, or a life that starts below its most
/// (09-zero-hour.md, Life is the original's; 09-zero-hour.md, A weapon converts when it hits one
/// target at once; 09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and
/// slow weapons).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{weapons} weapons, dropped {:?}, {armors} armors, {kinds} damage kinds, {unliving} unliving, {wounded} wounded",
    dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct FoughtCounts {
    pub(crate) weapons: usize,
    pub(crate) dropped: [usize; WeaponDropCause::ALL.len()],
    pub(crate) armors: usize,
    pub(crate) kinds: usize,
    pub(crate) unliving: usize,
    pub(crate) wounded: usize,
}

/// How many docks become nodes, supply centers drop-offs, gatherers gathers and hackers hacks,
/// and how many supply modules, or parts of one, the import drops for each cause, in the order of
/// `SupplyDropCause::ALL` (09-zero-hour.md, A box is money; 09-zero-hour.md, A hack is a channel
/// with no end).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{nodes} nodes, {drop_offs} drop-offs, {gathers} gathers, {hacks} hacks, dropped {:?}",
    dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SuppliedCounts {
    pub(crate) nodes: usize,
    pub(crate) drop_offs: usize,
    pub(crate) gathers: usize,
    pub(crate) hacks: usize,
    pub(crate) dropped: [usize; SupplyDropCause::ALL.len()],
}

/// How many trains and builds the import converts from every command set, and how many buttons it
/// drops for each cause, in the order of `ButtonDropCause::ALL` (09-zero-hour.md, Every command
/// set's trains and builds are imported).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display("{trains} trains, {builds} builds, dropped {:?}", dropped)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProducedCounts {
    pub(crate) trains: usize,
    pub(crate) builds: usize,
    pub(crate) dropped: [usize; ButtonDropCause::ALL.len()],
}

/// How many ground quads, overlays, texture copies and border wraps a terrain file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainDrawn {
    pub(crate) quads: usize,
    pub(crate) counts: TerrainCounts,
}

/// The fingerprints of an import: the imported package's and the rules package's, which a run
/// on Linux and a run on macOS give alike (09-zero-hour.md, One reproducible package;
/// 09-zero-hour.md, Fixtures in the tests, the install on request).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display("the fingerprints {game} and rules {rules}")]
#[serde(deny_unknown_fields)]
pub(crate) struct Fingerprints {
    pub(crate) game: PackageFingerprint,
    pub(crate) rules: PackageFingerprint,
}

impl ProductionSteps {
    /// The steps with the barracks' ticks `barracks_ticks`: the dozer's 5 s and the ranger's 5 s,
    /// the order's tick among them, and the money after each.
    pub(crate) const fn expected(barracks_ticks: u64) -> Self {
        Self {
            dozer_ticks: 5 * HZ,
            barracks_ticks,
            ranger_ticks: 5 * HZ,
            money: PRODUCTION_MONEY,
        }
    }
}

impl SupplySteps {
    /// The steps of `ticks`: the money after the buildings, and each delivery paying a truck's
    /// load.
    pub(crate) fn expected(ticks: SupplyTicks) -> Self {
        let mut paid = SUPPLY_MONEY;
        Self {
            built: ticks.built,
            built_money: SUPPLY_MONEY,
            deliveries: ticks.deliveries.map(|ticks| {
                paid += TRUCK_LOAD;
                SupplyDelivery { ticks, money: paid }
            }),
        }
    }

    /// Its ticks no closed form gives.
    pub(crate) fn ticks(&self) -> SupplyTicks {
        SupplyTicks {
            built: self.built,
            deliveries: self.deliveries.map(|delivery| delivery.ticks),
        }
    }
}

impl FactorySteps {
    /// The steps of the buildings' ticks `ticks`, and the money after the last.
    pub(crate) const fn expected(ticks: [u64; 3]) -> Self {
        Self {
            ticks,
            money: FACTORY_MONEY,
        }
    }
}

impl FightSteps {
    /// The fight of `ticks`: `CRUSADERS` trains of 10 s one after another; the money after the
    /// factory less each crusader's 900; and no end of the match, as the bot's other buildings
    /// count for its victory (09-zero-hour.md, Defeat by the last building that counts).
    pub(crate) fn expected(ticks: SkirmishTicks) -> Self {
        Self {
            train_ticks: u64::from(CRUSADERS) * 10 * HZ,
            shot_ticks: ticks.shot,
            hit_ticks: ticks.hit,
            fight_ticks: ticks.fight,
            money: FACTORY_MONEY - i64::from(CRUSADERS) * 900,
            result: None,
        }
    }
}

impl TerrainDrawn {
    /// A terrain file of `counts` on a map of `heights` samples: a ground quad for each cell the
    /// samples bound.
    pub(crate) fn expected(heights: [u32; 2], counts: TerrainCounts) -> Self {
        let cells = heights.map(|samples| {
            usize::try_from(samples.saturating_sub(1)).expect("a map's cells fit a usize")
        });
        Self {
            quads: cells[0] * cells[1],
            counts,
        }
    }
}
