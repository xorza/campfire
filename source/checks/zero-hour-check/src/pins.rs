//! What the check expects: what the game's data gives by a closed form, and the shapes of what
//! it measures, which `golden.toml` pins.

use campfire_bots::OrderUnsent;
use campfire_client_files::{ClientUnits, StateAnimation};
use campfire_common::{DeclaredName, PackageFingerprint};
use campfire_import::{
    AnimationDropCause, ButtonDropCause, StateDropCause, SupplyDropCause, UpgradeDropCause,
    WeaponDropCause,
};
use campfire_mode::MatchResult;
use campfire_orders::OrderDropCause;
use campfire_runner::BotInputDrop;
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

/// The money after the crusaders' trains, each 900, the strategy center's 2,500, and the
/// researches' 2,000, 1,000, 800 and 500.
const UPGRADES_MONEY: i64 = FACTORY_MONEY - CRUSADERS_COST - 2500 - (2000 + 1000 + 800 + 500);

/// What `CRUSADERS` crusaders' trains cost, 900 each.
const CRUSADERS_COST: i64 = CRUSADERS as i64 * 900;

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

/// How many unit types' looks have a draw of more than one state, and client objects' looks
/// night or snow variants; how many unit types' looks have no draws, of objects that draw
/// nothing; how many states the unit types' looks hold, and how many of their conditions have no
/// test (02-engine-core.md, A look is states of conditions); how many maps are night maps and
/// snowy maps; how many animations the looks name the import converts, and drops for each
/// cause, in the order of `AnimationDropCause::ALL` (09-zero-hour.md, An animation is a glTF
/// file of its own); and how many states it drops for each cause, in the order of
/// `StateDropCause::ALL`.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{varied} varied looks, {empty} empty looks, {states} states, {never} untested conditions, \
     {night} night maps, {snowy} snowy maps, {animations} animations, dropped {:?}, dropped \
     states {:?}",
    dropped_animations,
    dropped_states
)]
#[serde(deny_unknown_fields)]
pub(crate) struct LookCounts {
    pub(crate) varied: usize,
    pub(crate) empty: usize,
    pub(crate) states: usize,
    pub(crate) never: usize,
    pub(crate) night: usize,
    pub(crate) snowy: usize,
    pub(crate) animations: usize,
    pub(crate) dropped_animations: [usize; AnimationDropCause::ALL.len()],
    pub(crate) dropped_states: [usize; StateDropCause::ALL.len()],
}

/// The animations the Ranger's look plays, by their files' names: as it stands, holding no
/// condition; as it walks, `MOVING`; as it fires its rifle, `FIRING_A` and `USING_WEAPON_A`; and
/// as it dies, `DYING`; each the clips of the state its first draw draws then, as the client
/// picks it (02-engine-core.md, A look is states of conditions).
#[derive(Debug, Display, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[display("stands {stands:?}, walks {walks:?}, fires {fires:?}, dies {dies:?}")]
#[serde(deny_unknown_fields)]
pub(crate) struct RangerLook {
    pub(crate) stands: Vec<String>,
    pub(crate) walks: Vec<String>,
    pub(crate) fires: Vec<String>,
    pub(crate) dies: Vec<String>,
}

impl RangerLook {
    /// The animations the Ranger's look of `looks` plays as it stands, walks, fires and dies, as
    /// `SetMatch::best` picks each state.
    pub(crate) fn of(looks: &ClientUnits) -> Self {
        let ranger = DeclaredName::new("americainfantryranger").expect("a name");
        let draw = &looks.units[&ranger].draws[0];
        let clips = |held: &[&str]| -> Vec<String> {
            let state = &draw.states()[draw.best_state(|name| held.contains(&name.as_str()))];
            let clips = state.animation.iter().flat_map(StateAnimation::clips);
            let files = clips.map(|clip| clip.animation.as_str());
            let names = files.map(|file| {
                let file = file.trim_start_matches("client/animations/");
                file.trim_end_matches(".glb").to_owned()
            });
            names.collect()
        };
        Self {
            stands: clips(&[]),
            walks: clips(&["moving"]),
            fires: clips(&["firing_a", "using_weapon_a"]),
            dies: clips(&["dying"]),
        }
    }
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

/// The upgrade step's measures (09-zero-hour.md, Zero Hour's upgrade flags are tags and stats): the
/// ticks from the researches' orders to the last one held; the money left; the crusaders' most
/// life, in whole points, before and after Composite Armor; whether the ranger holds the tags of
/// its armor set's and its weapon set's `PLAYER_UPGRADE` after Chemical Suits and the flash bang;
/// how many of its weapons it may use before and after them; and whether the power plant holds its
/// own control rods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UpgradeSteps {
    pub(crate) research_ticks: u64,
    pub(crate) money: i64,
    pub(crate) crusader_life: [i64; 2],
    pub(crate) ranger_tags: [bool; 2],
    pub(crate) ranger_weapons: [usize; 2],
    pub(crate) rods: bool,
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
/// result, if it did, the buildings each player slot held, each whole and alive, the most
/// operations one think of its bots ran, which the mode's call limit bounds, and the times a walker
/// stood stuck 1 s, which crowds that yield keep few (09-zero-hour.md, A bots' match that ends;
/// 09-zero-hour.md, The bots' limits are measured).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BotMatchEnd {
    pub(crate) ended: Option<u64>,
    pub(crate) result: Option<MatchResult>,
    pub(crate) standing: [usize; 2],
    pub(crate) think_ops: u64,
    pub(crate) stalls: u64,
    pub(crate) drops: [BotDrops; 2],
}

/// The orders of a player slot's bot that came to nothing over a play: those its search left
/// unsent, the inputs the session log refused, the commands nothing read (02-engine-core.md, A
/// command nothing reads is counted), the orders no unit took, and each unit's drop of an order,
/// by its cause, as an order that no unit takes shows only as a count (09-zero-hour.md,
/// What a play drops is counted).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BotDrops {
    pub(crate) unsent: UnsentCounts,
    pub(crate) refused: RefusedCounts,
    pub(crate) unread: u64,
    pub(crate) untaken: u64,
    pub(crate) dropped: DroppedCounts,
}

/// The orders a bot's search left unsent, by `OrderUnsent`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnsentCounts {
    pub(crate) no_avatar: u64,
    pub(crate) no_unit: u64,
    pub(crate) no_slot: u64,
    pub(crate) no_target: u64,
    pub(crate) no_place: u64,
    pub(crate) no_stack: u64,
}

/// The bot's inputs the session log refused, which the server drops, by `BotInputDrop`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RefusedCounts {
    pub(crate) ahead_of_time: u64,
    pub(crate) log_full: u64,
}

/// The units' drops of orders, by `OrderDropCause`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DroppedCounts {
    pub(crate) no_unit: u64,
    pub(crate) no_walker: u64,
    pub(crate) no_weapon: u64,
    pub(crate) hidden: u64,
    pub(crate) slot_kind: u64,
    pub(crate) no_build: u64,
    pub(crate) unusable: u64,
}

impl UnsentCounts {
    /// Counts one order left unsent for `cause`.
    pub(crate) const fn count(&mut self, cause: OrderUnsent) {
        let counted = match cause {
            OrderUnsent::NoAvatar => &mut self.no_avatar,
            OrderUnsent::NoUnit => &mut self.no_unit,
            OrderUnsent::NoSlot => &mut self.no_slot,
            OrderUnsent::NoTarget => &mut self.no_target,
            OrderUnsent::NoPlace => &mut self.no_place,
            OrderUnsent::NoStack => &mut self.no_stack,
        };
        *counted += 1;
    }
}

impl RefusedCounts {
    /// Counts one input refused for `cause`.
    pub(crate) const fn count(&mut self, cause: BotInputDrop) {
        let counted = match cause {
            BotInputDrop::AheadOfTime => &mut self.ahead_of_time,
            BotInputDrop::LogFull => &mut self.log_full,
        };
        *counted += 1;
    }
}

impl DroppedCounts {
    /// Counts one unit's drop for `cause`.
    pub(crate) const fn count(&mut self, cause: OrderDropCause) {
        let counted = match cause {
            OrderDropCause::NoUnit => &mut self.no_unit,
            OrderDropCause::NoWalker => &mut self.no_walker,
            OrderDropCause::NoWeapon => &mut self.no_weapon,
            OrderDropCause::Hidden => &mut self.hidden,
            OrderDropCause::SlotKind => &mut self.slot_kind,
            OrderDropCause::NoBuild => &mut self.no_build,
            OrderDropCause::Unusable => &mut self.unusable,
        };
        *counted += 1;
    }
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
    pub(crate) strategy_center: u64,
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

/// How many researches the import converts from every command set, how many upgrade modules its
/// unit types take, and how many modules and objects of sets it drops for each cause, in the order
/// of `UpgradeDropCause::ALL` (09-zero-hour.md, Zero Hour's upgrade flags are tags and stats).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display("{researches} researches, {modules} modules, dropped {:?}", dropped)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpgradedCounts {
    pub(crate) researches: usize,
    pub(crate) modules: usize,
    pub(crate) dropped: [usize; UpgradeDropCause::ALL.len()],
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
    /// upgrades; and no end of the match, as the bot's other buildings count for its victory
    /// (09-zero-hour.md, Defeat by the last building that counts).
    pub(crate) fn expected(ticks: SkirmishTicks) -> Self {
        Self {
            train_ticks: u64::from(CRUSADERS) * 10 * HZ,
            shot_ticks: ticks.shot,
            hit_ticks: ticks.hit,
            fight_ticks: ticks.fight,
            money: UPGRADES_MONEY,
            result: None,
        }
    }
}

impl UpgradeSteps {
    /// The step's measures: the strategy center's two researches of 60 s one after the other in its
    /// queue, the second ordered a tick after the first, and the barracks' and the power plant's of
    /// 30 s beside them, so the last ends in 120 s, the first order's tick among them; the money
    /// after them; the crusader's 480 life and 680 after Composite Armor's two modules of 100, a
    /// tick after the last; the ranger's tags of neither flag and then of both; its rifle alone and
    /// then its rifle and its flash bang; and the rods held.
    pub(crate) const fn expected() -> Self {
        Self {
            research_ticks: 120 * HZ,
            money: UPGRADES_MONEY,
            crusader_life: [480, 680],
            ranger_tags: [false, true],
            ranger_weapons: [1, 2],
            rods: true,
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
