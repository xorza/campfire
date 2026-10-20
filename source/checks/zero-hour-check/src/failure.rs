use campfire_bots::{BotScriptError, OrderScriptError};
use campfire_math::{Num, Vec3};
use campfire_mode::PickError;
use campfire_orders::OrdersDirective;
use derive_more::Display;

use crate::pins::{
    BotBase, FactorySteps, FightSteps, Fingerprints, FoughtCounts, LookCounts, MapCounts,
    ProducedCounts, ProductionSteps, TerrainDrawn,
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
    /// The import's looks of night and snow, its empty looks, and its maps of night and snow differ
    /// from the pinned.
    #[display("it has {_0}")]
    Looks(Box<Differs<LookCounts>>),
    /// The import's trains, builds and dropped buttons by cause differ from the pinned.
    #[display("it converted {_0}")]
    Produced(Box<Differs<ProducedCounts>>),
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
    /// The factory's chain's ticks or money differ from the expected.
    #[display("it built the factory's chain as {_0}")]
    Factory(Box<Differs<FactorySteps>>),
    /// The mode takes no pick the play's host gives of its options.
    #[display("the mode takes no pick of {_0}")]
    Options(PickError),
    /// The builder bot's orders file is no order script.
    #[display("the builder bot's orders: {_0}")]
    BotOrders(OrderScriptError),
    /// The builder bot's orders name what the mode lacks.
    #[display("the builder bot's orders: {_0}")]
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
