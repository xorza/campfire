//! What the check expects: what the game's data gives by a closed form, and the shapes of what
//! it measures, which `golden.toml` pins.

use campfire_bots::OrderUnsent;
use campfire_client_files::{ClientUnits, StateAnimation};
use campfire_common::{DeclaredName, PackageFingerprint};
use campfire_import::{
    AnimationDropCause, ButtonDropCause, ContainDropCause, EffectDropCause, PanelDropCause,
    PowerDropCause, RoadDropCause, SoundDropCause, StateDropCause, SupplyDropCause,
    UpgradeDropCause, WeaponDropCause,
};
use campfire_math::Num;
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

/// What the power plant's sale gives back: its 800 times `GameData.ini`'s `SellPercentage` of 50 %,
/// as it gives no `RefundValue` (04-capabilities/production.md, A building may be sold).
const SALE_REFUND: i64 = 800 / 2;
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
/// objects, and their looks; its markers; its walls, its cliff cells grown by one ring and
/// merged row by row (09-zero-hour.md, Cliffs are walls); and its grid's cells, as columns and
/// rows, and the unit types of a sight (09-zero-hour.md, Zero Hour sees by the vision capability);
/// the unit types on a track of veterancy, and the trainable objects that gain no level
/// (generals.md, Veterancy is progression's data); and the mode's perks, those of a cost, and the
/// perks the factions start with, summed over the factions (generals.md, A science is a perk of a
/// player).
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
    pub(crate) grid: [usize; 2],
    pub(crate) seers: usize,
    pub(crate) trainable: usize,
    pub(crate) untrainable: usize,
    pub(crate) perks: usize,
    pub(crate) learnable: usize,
    pub(crate) granted: usize,
}

/// A terrain file's overlays; its decals' triangles; a copy for each tile its classes' textures
/// load and for each road type's texture; and a border wrap for each class's block and each road
/// type's texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerrainCounts {
    pub(crate) overlays: usize,
    pub(crate) decals: usize,
    pub(crate) copies: usize,
    pub(crate) wraps: usize,
}

/// Tournament Desert's road segments, curves, miters, junctions by kind and alpha joins the import
/// draws, and the segments it drops for each cause, in the order of `RoadDropCause::ALL`
/// (09-zero-hour.md, A road is a decal the import meshes).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{segments} segments, {curves} curves, {miters} miters, {tees} tees, {four_ways} four-ways, \
     {ys} Ys, {hs} Hs, {joins} joins, dropped {:?}",
    dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct RoadCounts {
    pub(crate) segments: usize,
    pub(crate) curves: usize,
    pub(crate) miters: usize,
    pub(crate) tees: usize,
    pub(crate) four_ways: usize,
    pub(crate) ys: usize,
    pub(crate) hs: usize,
    pub(crate) joins: usize,
    pub(crate) dropped: [usize; RoadDropCause::ALL.len()],
}

/// How many unit types' looks have a draw of more than one state, and client objects' looks night
/// or snow variants; how many unit types' looks have no draws, of objects that draw nothing; how
/// many states the unit types' looks hold, and how many of their sets count conditions no test says
/// (02-engine-core.md, A set counts the conditions no test says); how many maps are night maps and
/// snowy maps; how many animations the looks name the import converts, and drops for each cause, in
/// the order of `AnimationDropCause::ALL` (09-zero-hour.md, An animation is a glTF file of its
/// own); and how many states it drops for each cause, in the order of `StateDropCause::ALL`.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{varied} varied looks, {empty} empty looks, {states} states, {untested} untested sets, \
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
    pub(crate) untested: usize,
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
        let draw = &looks.units()[&ranger].draws[0];
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
/// result, if it did, the buildings each player slot held, each whole and alive, each player's
/// rank and the perks it held, its faction's among them (generals.md, A bot buys its skill set),
/// the most operations one think of its bots ran, which the mode's call limit bounds, the times a walker
/// stood stuck 1 s, which crowds that yield keep few, its bots' drops, and the cells sent each
/// bot's player (09-zero-hour.md, A bots' match that ends; 09-zero-hour.md, The bots' limits are
/// measured).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BotMatchEnd {
    pub(crate) ended: Option<u64>,
    pub(crate) result: Option<MatchResult>,
    pub(crate) standing: [usize; 2],
    pub(crate) ranks: [u32; 2],
    pub(crate) perks: [usize; 2],
    pub(crate) think_ops: u64,
    pub(crate) stalls: u64,
    pub(crate) drops: [BotDrops; 2],
    pub(crate) sight: SightSent,
}

/// What the server sent each bot's player of its team's cells over a play: the changes of those it
/// sees and of those it explored, each sent whole, and the words of one, of 8 bytes, which measure
/// what the kind `sight` costs (04-capabilities/vision.md, The server sends a team's cells to its
/// player).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SightSent {
    pub(crate) seen: [u64; 2],
    pub(crate) explored: [u64; 2],
    pub(crate) words: usize,
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

/// The sale step's measures: the ticks from the power plant's sale's order to the plant gone, the
/// order's own among them, and the money it gave back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SaleSteps {
    pub(crate) ticks: u64,
    pub(crate) refund: i64,
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

/// What a delivery of a Chinook pays: its `MaxBoxes` of 8 boxes of `ValuePerSupplyBox`, 75.
const CHINOOK_LOAD: i64 = 8 * 75;

/// The riders a Troop Crawler comes with, its `InitialPayload` of 8 Red Guards.
const CRAWLER_LOAD: usize = 8;

/// The helicopter play's steps: its ticks, and what the Chinook's first load paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HelicopterSteps {
    pub(crate) ticks: HelicopterTicks,
    pub(crate) load: i64,
}

/// The ticks of the helicopter play's steps no closed form gives: the Comanche's from its start
/// to its height, the Chinook's from its gather's order to its first load home, and the
/// Comanche's from its flight's order to its fall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HelicopterTicks {
    pub(crate) climbed: u64,
    pub(crate) delivered: u64,
    pub(crate) shot_down: u64,
}

/// The combat play's steps (combat-response.md, Combat chooses the weapon; combat-response.md, A
/// turret has its own target; combat-response.md, A retaliation's target is chased to its leash):
/// its ticks; the slot of the weapon the Comanche fired first at the Rebel and then at the
/// Scorpion; how far each Crusader that answered the buggy stood, back, from where it
/// stood when called; whether the driving Crusader's first hit on the Rebel came from its turret
/// as it drove; and how far from its goal it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CombatSteps {
    pub(crate) ticks: CombatTicks,
    pub(crate) weapons: [u8; 2],
    pub(crate) strays: [Num; 2],
    pub(crate) drove: bool,
    pub(crate) left: Num,
}

/// The ticks of the combat play's steps no closed form gives: the Comanche's first shot at the
/// Rebel and at the Scorpion, each from its order; the first Crusader's, from the buggy's order;
/// each Crusader's walk back, from that hit until it stands with no target; and the driven
/// Crusader's first hit on the Rebel and its stop, each from the order of its drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatTicks {
    pub(crate) cannon: u64,
    pub(crate) missiles: u64,
    pub(crate) rockets: u64,
    pub(crate) back: [u64; 2],
    pub(crate) turret: u64,
    pub(crate) arrived: u64,
}

/// The transport play's steps (transports.md, A container says who rides; transports.md, A
/// container may let its riders fire; transports.md, A container's fall hurts its riders;
/// transports.md, A flier lands for its riders): its ticks; whether the rider that fired first
/// fired from inside the Humvee; how many of the Troop Crawler's riders stood out of it as it fell,
/// and whether each lost its share of its life; and whether the Chinook's riders stood alive
/// within reach of its goal once out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportSteps {
    pub(crate) ticks: TransportTicks,
    pub(crate) fired_inside: bool,
    pub(crate) out: usize,
    pub(crate) hurt: bool,
    pub(crate) near: bool,
}

/// The ticks of the transport play's steps no closed form gives, each from its order: the
/// Chinook's boarding, its flight, and its riders' exit; the Humvee's boarding; a rider's first
/// shot from it, from the Humvee's drive; the Troop Crawler's fall, from the Crusaders' attack; and
/// the Humvee's evacuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TransportTicks {
    pub(crate) loaded: u64,
    pub(crate) flown: u64,
    pub(crate) landed: u64,
    pub(crate) boarded: u64,
    pub(crate) fired: u64,
    pub(crate) fell: u64,
    pub(crate) evacuated: u64,
}

/// The garrison play's steps (transports.md, A free building takes its riders' side;
/// transports.md, A container's modifier holds on its riders; transports.md, A container's fall
/// hurts its riders): its ticks; whether the building turned its Rebels' player's; whether the
/// Ranger took their fire before it fired; whether the Rebels left at the building's share of life
/// of leaving or below, while it stood; and whether it stood free once they left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GarrisonSteps {
    pub(crate) ticks: GarrisonTicks,
    pub(crate) turned: bool,
    pub(crate) first: bool,
    pub(crate) held: bool,
    pub(crate) freed: bool,
}

/// The ticks of the garrison play's steps no closed form gives, each from its order: the Rebels'
/// garrison, the Ranger's first hit, from its attack, and the Rebels' leave, from the Crusaders'
/// attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GarrisonTicks {
    pub(crate) garrisoned: u64,
    pub(crate) outranged: u64,
    pub(crate) left: u64,
}

/// The science play's steps (generals.md, A science is a perk of a player): how many Paladins the
/// war factory queued or made before the player held their science, the points left once it is
/// learned, and the ticks the Paladin took from its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScienceSteps {
    pub(crate) queued_before: usize,
    pub(crate) points: u32,
    pub(crate) paladin: u64,
}

/// The ticks of the jet play's steps no closed form gives, each from the one before: the Raptor's
/// train, from its order; its takeoff, from its order to fly over the target; its flight there;
/// its first hit, from its attack's order; its way home;
/// its landing, at its place; and its reload; and the target's life after them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JetTicks {
    pub(crate) trained: u64,
    pub(crate) took_off: u64,
    pub(crate) over: u64,
    pub(crate) hit: u64,
    pub(crate) home: u64,
    pub(crate) parked: u64,
    pub(crate) reloaded: u64,
    pub(crate) target_life: i64,
}

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
/// how many unit types' objects it gives no life, or a life that starts below its most, how many
/// weapons prefer a target, how many lose the preference of a kind of no tag, and how many unit
/// types answer an ally's call (combat-response.md, The import converts what the choice and the
/// answer read; 09-zero-hour.md,
/// Life is the original's; 09-zero-hour.md, A weapon converts when it hits one
/// target at once; 09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and
/// slow weapons).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{weapons} weapons, dropped {:?}, {armors} armors, {kinds} damage kinds, {unliving} unliving, {wounded} wounded, {preferring} preferring, {unpreferred} unpreferred, {retaliating} retaliating",
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
    pub(crate) preferring: usize,
    pub(crate) unpreferred: usize,
    pub(crate) retaliating: usize,
}

/// How many unit types the import makes movers of each appearance, treads, two legs, four wheels
/// and a motorcycle, how many of them a hurt slows, how many modifiers the hurts share, how many
/// wheels turn at the world's bound, of a locomotor of no `MinTurnSpeed`, and how many push
/// themselves out of no cell, as a dozer does not (driving.md, Each appearance moves as its mover;
/// driving.md, A hurt type may hold a modifier).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "movers {:?}, {hurt} hurt, {modifiers} hurt modifiers, {unturning} wheels of no turning speed, {uncorrected} uncorrected",
    movers
)]
#[serde(deny_unknown_fields)]
pub(crate) struct DrivenCounts {
    pub(crate) movers: [usize; 4],
    pub(crate) hurt: usize,
    pub(crate) modifiers: usize,
    pub(crate) unturning: usize,
    pub(crate) uncorrected: usize,
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

/// How many unit types the import makes transports, of an `enter` that counts its riders' sizes,
/// and garrisons, of one that counts each rider one; how many garrisons fire from their points;
/// how many types ride; and how many contain modules, or parts of one, it drops for each cause, in
/// the order of `ContainDropCause::ALL` (transports.md, A container says who rides).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{transports} transports, {garrisons} garrisons, {fire_points} of fire points, {riders} riders, \
     dropped {:?}",
    dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContainedCounts {
    pub(crate) transports: usize,
    pub(crate) garrisons: usize,
    pub(crate) fire_points: usize,
    pub(crate) riders: usize,
    pub(crate) dropped: [usize; ContainDropCause::ALL.len()],
}

/// How many particle systems and effect lists the import converts, how many textures the systems
/// draw; how many actions name a fire effect and a hit effect, how many unit looks a trail, an end
/// and a death effect, and how many look states a weapon slot's fire bone; and how many systems and
/// lists it drops one or more parts of for each cause, in the order of `EffectDropCause::ALL`
/// (09-zero-hour.md, Particles and effects).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{systems} systems, {lists} lists, {textures} textures, {fires} fires, {hits} hits, \
     {trails} trails, {ends} ends, {deaths} deaths, {bones} states of fire bones, dropped {:?}",
    dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct EffectCounts {
    pub(crate) systems: usize,
    pub(crate) lists: usize,
    pub(crate) textures: usize,
    pub(crate) fires: usize,
    pub(crate) hits: usize,
    pub(crate) trails: usize,
    pub(crate) ends: usize,
    pub(crate) deaths: usize,
    pub(crate) bones: usize,
    pub(crate) dropped: [usize; EffectDropCause::ALL.len()],
}

/// How many actions the import gives an icon and a key, how many unit looks place buttons on
/// their panels and how many buttons they place, how many commands the grid gives an icon, and
/// how many buttons it drops or shows in part for each cause, in the order of
/// `PanelDropCause::ALL` (02-engine-core.md, A look places its buttons).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{icons} icons, {keys} keys, {panels} panels, {places} places, {commands} commands, \
     dropped {:?}",
    dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct PanelCounts {
    pub(crate) icons: usize,
    pub(crate) keys: usize,
    pub(crate) panels: usize,
    pub(crate) places: usize,
    pub(crate) commands: usize,
    pub(crate) dropped: [usize; PanelDropCause::ALL.len()],
}

/// How many sounds the import converts, how many effect lists, actions' fires and looks name
/// them, how many files the sounds and the music play, how many playlists the music holds, and how
/// many events it drops or plays in part for each cause, in the order of `SoundDropCause::ALL`
/// (02-engine-core.md, An event is the game's, as data).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{sounds} sounds, {lists} lists, {fires} fires, {voiced} voiced looks, {files} files, \
     {tracks} tracks, {playlists} playlists, dropped {:?}",
    dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SoundCounts {
    pub(crate) sounds: usize,
    pub(crate) lists: usize,
    pub(crate) fires: usize,
    pub(crate) voiced: usize,
    pub(crate) files: usize,
    pub(crate) tracks: usize,
    pub(crate) playlists: usize,
    pub(crate) dropped: [usize; SoundDropCause::ALL.len()],
}

/// How many trains and builds the import converts from every command set, how many unit types it
/// gives a sale, how many buttons it drops for each cause, in the order of `ButtonDropCause::ALL`
/// (09-zero-hour.md, Every command set's trains and builds are imported), how many powers it
/// converts to casts, and how many power buttons it drops for each cause, in the order of
/// `PowerDropCause::ALL` (generals.md, A power is a cast of its building).
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[display(
    "{trains} trains, {builds} builds, {sells} sales, dropped {:?}, {casts} casts, dropped {:?}",
    dropped,
    powers_dropped
)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProducedCounts {
    pub(crate) trains: usize,
    pub(crate) builds: usize,
    pub(crate) sells: usize,
    pub(crate) dropped: [usize; ButtonDropCause::ALL.len()],
    pub(crate) casts: usize,
    pub(crate) powers_dropped: [usize; PowerDropCause::ALL.len()],
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

/// How many ground quads, overlays, decal triangles, texture copies and border wraps a terrain
/// file holds.
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

impl CombatSteps {
    /// The steps of `ticks` when the Comanche attacks the Rebel and the Scorpion with the weapons
    /// of the slots `weapons`, each Crusader walks back to the point it left, and the driven one
    /// fires as it drives and stops at its goal.
    pub(crate) const fn expected(ticks: CombatTicks, weapons: [u8; 2]) -> Self {
        Self {
            ticks,
            weapons,
            strays: [Num::ZERO; 2],
            drove: true,
            left: Num::ZERO,
        }
    }
}

impl TransportSteps {
    /// The steps of `ticks` when a rider fires from inside the Humvee, the Troop Crawler's 8
    /// riders stand out of it as it falls, each hurt by its share, and the Chinook's riders stand
    /// by its goal.
    pub(crate) const fn expected(ticks: TransportTicks) -> Self {
        Self {
            ticks,
            fired_inside: true,
            out: CRAWLER_LOAD,
            hurt: true,
            near: true,
        }
    }
}

impl GarrisonSteps {
    /// The steps of `ticks` when the building turns its Rebels' player's, they outrange the
    /// Ranger, and they leave it at its share of leaving, free again.
    pub(crate) const fn expected(ticks: GarrisonTicks) -> Self {
        Self {
            ticks,
            turned: true,
            first: true,
            held: true,
            freed: true,
        }
    }
}

impl HelicopterSteps {
    /// The steps of `ticks`, a full load paying a Chinook's boxes.
    pub(crate) const fn expected(ticks: HelicopterTicks) -> Self {
        Self {
            ticks,
            load: CHINOOK_LOAD,
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

impl SaleSteps {
    /// The sale's measures: its 6 s, which start in its order's tick and end as the tick at their
    /// end runs, so the plant goes a tick past them; and `SALE_REFUND`, paid as it goes.
    pub(crate) const fn expected() -> Self {
        Self {
            ticks: 6 * HZ + 1,
            refund: SALE_REFUND,
        }
    }
}

impl FightSteps {
    /// The fight of `ticks`: `CRUSADERS` trains of 10 s one after another; the money after the
    /// upgrades and the sale; and no end of the match, as the bot's other buildings count for its
    /// victory (09-zero-hour.md, Defeat by the last building that counts).
    pub(crate) fn expected(ticks: SkirmishTicks) -> Self {
        Self {
            train_ticks: u64::from(CRUSADERS) * 10 * HZ,
            shot_ticks: ticks.shot,
            hit_ticks: ticks.hit,
            fight_ticks: ticks.fight,
            money: UPGRADES_MONEY + SALE_REFUND,
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
