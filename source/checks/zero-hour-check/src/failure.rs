use campfire_bots::{BotScriptError, OrderScriptError, ThinkError};
use campfire_common::{DeclaredName, ErrorReport};
use campfire_harness::{ProbeRefusal, ProbeWriteError};
use campfire_math::{Num, Vec3};
use campfire_mode::PickError;
use campfire_orders::OrdersDirective;
use derive_more::Display;

use crate::pins::{
    BotBase, BotMatchEnd, DefenseTicks, FactorySteps, FightSteps, Fingerprints, FoughtCounts,
    LookCounts, MapCounts, MissileTicks, ProducedCounts, ProductionSteps, RangerLook,
    SuppliedCounts, SupplySteps, TerrainDrawn, UpgradeSteps, UpgradedCounts,
};

/// What the import holds other than the original gives it: what Tournament Desert holds, or a
/// map that does not load in the imported mode.
#[derive(Debug, Display)]
pub(crate) enum CheckFailure {
    /// The import's fingerprints differ from the pinned.
    #[display("it gave {_0}")]
    Fingerprints(Box<Differs<Fingerprints>>),
    /// Tournament Desert's heights, units, client objects, markers or walls differ from the
    /// pinned.
    #[display("its map holds {_0}")]
    Map(Box<Differs<MapCounts>>),
    /// Its terrain file holds other than its expected quads, overlays, copies and wraps.
    #[display("its terrain holds {_0}")]
    Terrain(Box<Differs<TerrainDrawn>>),
    /// Cells it draws have a tile with no texture.
    #[display("{_0} of its drawn cells have a tile with no texture")]
    Untextured(usize),
    /// The import's varied looks, its empty looks, its states, its conditions of no test, its maps
    /// of night and snow, and its animations converted and dropped by cause differ from the
    /// pinned.
    #[display("it has {_0}")]
    Looks(Box<Differs<LookCounts>>),
    /// The animations the Ranger's look plays differ from the pinned.
    #[display("its Ranger plays {_0}")]
    Ranger(Box<Differs<RangerLook>>),
    /// The import's trains, builds and dropped buttons by cause differ from the pinned.
    #[display("it converted {_0}")]
    Produced(Box<Differs<ProducedCounts>>),
    /// The import's researches, upgrade modules and dropped modules by cause differ from the
    /// pinned.
    #[display("it upgraded {_0}")]
    Upgraded(Box<Differs<UpgradedCounts>>),
    /// The import's nodes, drop-offs, gathers, hacks and dropped supply by cause differ from the
    /// pinned.
    #[display("it supplied {_0}")]
    Supplied(Box<Differs<SuppliedCounts>>),
    /// The imported mode has no unit type a play reads.
    #[display("the imported mode has no unit type {_0}")]
    NoType(&'static str),
    /// A unit type of the imported mode lacks an action a play orders.
    #[display("the unit type {unit} has no action {action}")]
    NoAction {
        unit: &'static str,
        action: &'static str,
    },
    /// The imported mode has no player resource `money` for the production play to pay with.
    #[display("the imported mode has no money")]
    NoMoney,
    /// Tournament Desert's player 1 lacks a unit the production play orders.
    #[display("player 1 lacks a unit the production play orders")]
    ProductionStart,
    /// A step of the production play does not end within its limit.
    #[display("the production step {_0:?} does not end")]
    ProductionStep(OrdersDirective),
    /// The production play's ticks or money differ from the expected.
    #[display("it played production as {_0}")]
    Production(Box<Differs<ProductionSteps>>),
    /// The upgrade step's ticks, money, life, tags, weapons or object upgrade differ from the
    /// expected.
    #[display("it upgraded as {_0}")]
    Upgrades(Box<Differs<UpgradeSteps>>),
    /// The factory's chain's ticks or money differ from the expected.
    #[display("it built the factory's chain as {_0}")]
    Factory(Box<Differs<FactorySteps>>),
    /// The mode takes no pick the play's host gives of its options.
    #[display("the mode takes no pick of {_0}")]
    Options(PickError),
    /// A bot's orders file is no order script.
    #[display("a bot's orders: {_0}")]
    BotOrders(OrderScriptError),
    /// A bot's orders name what the mode lacks.
    #[display("a bot's orders: {_0}")]
    BotNames(BotScriptError),
    /// The bot's base differs from the pinned.
    #[display("the bot built {_0}")]
    Bot(Box<Differs<BotBase>>),
    /// The bot lacks the command center or the dozer the fight reads.
    #[display("the bot lacks its command center or its dozer")]
    FightStart,
    /// The fight does not end the match within its limit.
    #[display("the fight does not end the match")]
    NoEnd,
    /// The supply play's ticks or money differ from the expected.
    #[display("it played supply as {_0}")]
    Supply(Box<Differs<SupplySteps>>),
    /// The Gattling Cannon's defense play's ticks differ from the pinned.
    #[display("it defended as {_0}")]
    Defense(Box<Differs<DefenseTicks>>),
    /// The Patriot battery's defense play's ticks differ from the pinned.
    #[display("its Patriot battery defended as {_0}")]
    Patriot(Box<Differs<DefenseTicks>>),
    /// The Stinger site's defense play's ticks differ from the pinned.
    #[display("its Stinger site defended as {_0}")]
    Stinger(Box<Differs<DefenseTicks>>),
    /// The cannon's first hits came apart otherwise than the original's.
    #[display("the cannon's hits came apart as {_0}")]
    SpinUp(Box<Differs<[u64; 7]>>),
    /// A defense did not kill the bot's dozer within the limit.
    #[display("a defense did not kill the dozer within the limit")]
    Undefended,
    /// The missile play's ticks differ from the pinned.
    #[display("it fired missiles as {_0}")]
    Missile(Box<Differs<MissileTicks>>),
    /// The bot's dozer took damage before a steered missile flew.
    #[display("the dozer took damage before a steered missile flew")]
    Unsteered,
    /// The Tank Hunter did not kill the bot's dozer within the limit.
    #[display("the Tank Hunter did not kill the dozer within the limit")]
    NoMissileKill,
    /// The supply center freed no truck as it completed.
    #[display("the supply center freed no truck")]
    NoTruck,
    /// The free truck delivered fewer loads than the supply play pins within its limit.
    #[display("the free truck delivered {_0} loads within the limit")]
    NoDelivery(usize),
    /// The fight's ticks, money or end differ from the expected.
    #[display("it fought as {_0}")]
    Fight(Box<Differs<FightSteps>>),
    /// The import's weapons, armors, damage kinds and bodies differ from the pinned.
    #[display("it converted {_0}")]
    Fought(Box<Differs<FoughtCounts>>),
    /// Tournament Desert lacks a start of player 1 or 2.
    #[display("it has no start of player 1 or of player 2")]
    NoStarts,
    /// Player 1 starts with other than one command center and one dozer.
    #[display(
        "player 1 starts with {buildings} command centers and {dozers} dozers, not one of each"
    )]
    StartUnits { buildings: usize, dozers: usize },
    /// Player 1's command center stands elsewhere than its start, at this place.
    #[display("player 1's command center stands at ({}, {}), not at its start", _0.x, _0.z)]
    BuildingPlace(Vec3),
    /// Player 1's dozer stands in its command center's box, at this place.
    #[display("player 1's dozer stands in its command center's box, at ({}, {})", _0.x, _0.z)]
    DozerInBuilding(Vec3),
    /// The dozer stood in a wall in this tick, at this place.
    #[display("the dozer stood in a wall in tick {tick}, at ({}, {})", position.x, position.z)]
    DozerInWall { tick: u64, position: Vec3 },
    /// The dozer's walk did not end within the check's limit of ticks.
    #[display("the dozer's walk did not end")]
    NeverArrived,
    /// The dozer's walk ended this far short of the other start.
    #[display("the dozer's walk ended {_0} m short of the other start")]
    DriveShort(Num),
    /// The dozer took other ticks to the other start than the pinned.
    #[display("the dozer took ticks to the other start: {_0}")]
    DriveTicks(Box<Differs<u64>>),
    /// The imported mode lacks a faction's base, its params, or an army in it.
    #[display("the imported mode has no base of {_0}")]
    NoBase(&'static str),
    /// The imported mode has no unit type of this name, which its base names.
    #[display("the imported mode has no unit type {_0}, which a base names")]
    BaseType(DeclaredName),
    /// A player slot of the base play holds none of a base building whole.
    #[display("player {} holds no {unit}", player_slot + 1)]
    BaseBuilding {
        player_slot: u32,
        unit: DeclaredName,
    },
    /// A player slot of the base play holds fewer of a base unit than its base's count.
    #[display("player {} holds {held} of {unit}", player_slot + 1)]
    BaseUnit {
        player_slot: u32,
        unit: DeclaredName,
        held: usize,
    },
    /// Bodies of the base play's player slots overlap a wall, or a body, this many times.
    #[display("the bases' bodies overlap {_0} times")]
    BaseOverlaps(usize),
    /// The imported mode names no bot of the name.
    #[display("the mode names no bot {_0}")]
    NoBot(&'static str),
    /// A think of a bot of the mode failed.
    #[display("a bot's think: {_0}")]
    BotThink(ThinkError),
    /// The base play's match stood otherwise than the pinned at its end or its limit.
    #[display("the bases' bots ended as {_0}")]
    Base(Box<Differs<BotMatchEnd>>),
    /// The attack play's match stood otherwise than the pinned at its end or its limit.
    #[display("the attacker and the builder ended as {_0}")]
    Attack(Box<Differs<BotMatchEnd>>),
    /// A play's probe that its match refuses.
    #[display("{_0}")]
    Probe(ProbeRefusal),
    /// A play's probe whose file does not write, with why.
    #[display("{}", ErrorReport::of(_0))]
    ProbeWrite(ProbeWriteError),
    /// A run that blesses failed before it measured every value the file pins, so it writes none.
    #[display("the run measured not every value it pins, so it writes no golden file")]
    Unmeasured,
}

/// What a check measured, and what it expected: the pinned value, or one of a closed form. A
/// failure boxes it, as it holds a measure twice.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display("{got:?}, not {expected:?}")]
pub(crate) struct Differs<T> {
    pub(crate) got: T,
    pub(crate) expected: T,
}

impl<T: PartialEq> Differs<T> {
    /// The difference of `got` from `expected`; none when they are one.
    pub(crate) fn of(got: T, expected: T) -> Option<Box<Self>> {
        (got != expected).then(|| Box::new(Self { got, expected }))
    }
}
