use campfire_common::MapName;
use campfire_log::ErrorReport;
use campfire_math::{Num, Vec3};
use campfire_orders::OrdersAction;
use campfire_package::LoadError;
use derive_more::Display;

use crate::pins::{
    CLIENT_OBJECTS, DRIVE_TICKS, FIGHT, FINGERPRINTS, FOUGHT, FightSteps, Fingerprints,
    FoughtCounts, HEIGHTS, LOOKS, LookCounts, MARKERS, OBJECT_LOOKS, PRODUCED, PRODUCTION,
    ProducedCounts, ProductionSteps, TERRAIN, TerrainDrawn, UNIT_TYPES, UNITS, WALLS,
};

/// What the import holds other than the original gives it: what Tournament Desert holds, or a
/// map that does not load in the imported mode.
#[derive(Debug, Display)]
pub(crate) enum Failure {
    /// The import's fingerprints differ from those the runs on Linux and on macOS gave.
    #[display("it gave {_0}, not {FINGERPRINTS}")]
    Fingerprints(Fingerprints),
    /// Its heights are not `HEIGHTS` samples.
    #[display("its heights are {columns} × {rows} samples, not {} × {}", HEIGHTS[0], HEIGHTS[1])]
    Heights { columns: u32, rows: u32 },
    /// It places other than `UNITS` units of `UNIT_TYPES` types.
    #[display("it places {units} units of {types} types, not {UNITS} of {UNIT_TYPES}")]
    Units { units: usize, types: usize },
    /// It has other than `CLIENT_OBJECTS` client objects of `OBJECT_LOOKS` looks.
    #[display(
        "it has {objects} client objects of {looks} looks, not {CLIENT_OBJECTS} of {OBJECT_LOOKS}"
    )]
    ClientObjects { objects: usize, looks: usize },
    /// It has other than `MARKERS` markers.
    #[display("it has {_0} markers, not {MARKERS}")]
    Markers(usize),
    /// Its terrain file holds other than its pinned quads, overlays, copies and wraps.
    #[display(
        "its terrain has {} quads, {} overlays, {} copies and {} wraps, not {}, {}, {} and {}",
        _0.quads, _0.overlays, _0.copies, _0.wraps,
        TERRAIN.quads, TERRAIN.overlays, TERRAIN.copies, TERRAIN.wraps
    )]
    Terrain(TerrainDrawn),
    /// Cells it draws have a tile with no texture.
    #[display("{_0} of its drawn cells have a tile with no texture")]
    Untextured(usize),
    /// Its grown cliffs merge into other than its pinned walls.
    #[display("it has {_0} walls, not {WALLS}")]
    Walls(usize),
    /// A map of the import does not load in the imported mode.
    #[display("map {map} does not load: {}", ErrorReport::of(error))]
    MapLoad { map: MapName, error: LoadError },
    /// The import's looks of night and snow, its empty looks, and its maps of night and snow differ
    /// from the pinned.
    #[display("it has {_0}, not {LOOKS}")]
    Looks(LookCounts),
    /// The import's trains, builds and dropped buttons by cause differ from the pinned.
    #[display("it converted {_0}, not {PRODUCED}")]
    Produced(ProducedCounts),
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
    ProductionStep(OrdersAction),
    /// The production play's ticks or money differ from the pinned.
    #[display("it played production as {_0:?}, not {PRODUCTION:?}")]
    Production(ProductionSteps),
    /// The bot lacks the command center or the dozer the fight reads.
    #[display("the bot lacks its command center or its dozer")]
    FightStart,
    /// The fight does not end the match within its limit.
    #[display("the fight does not end the match")]
    NoEnd,
    /// The fight's ticks, money or end differ from the pinned.
    #[display("it fought as {_0:?}, not {FIGHT:?}")]
    Fight(FightSteps),
    /// The import's weapons, armors, damage kinds and bodies differ from the pinned.
    #[display("it converted {_0}, not {FOUGHT}")]
    Fought(Box<FoughtCounts>),
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
    /// The dozer stood in a wall's cell in this tick, at this place.
    #[display("the dozer stood in a wall's cell in tick {tick}, at ({}, {})", pos.x, pos.z)]
    DozerInWall { tick: u64, pos: Vec3 },
    /// The dozer's walk did not end within the check's limit of ticks.
    #[display("the dozer's walk did not end")]
    NeverArrived,
    /// The dozer's walk ended this far short of the other start.
    #[display("the dozer's walk ended {_0} m short of the other start")]
    DriveShort(Num),
    /// The dozer took more ticks to the other start than the check pins.
    #[display("the dozer took {_0} ticks to the other start, not at most {DRIVE_TICKS}")]
    DriveTicks(u64),
}
