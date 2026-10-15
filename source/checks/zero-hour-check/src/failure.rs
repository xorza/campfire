use campfire_common::{Fingerprint, MapName};
use campfire_import::DropCause;
use campfire_log::ErrorReport;
use campfire_math::{Num, Vec3};
use campfire_orders::OrdersAction;
use campfire_package::LoadError;
use derive_more::Display;

use crate::{DRIVE_TICKS, FINGERPRINTS, LOOKS, PRODUCED, PRODUCTION, TERRAIN, WALLS};

/// What the import holds other than the original gives it: what Tournament Desert holds, or a
/// map that does not load in the imported mode.
#[derive(Debug, Display)]
pub(crate) enum Failure {
    /// The import's fingerprints differ from those the runs on Linux and on macOS gave.
    #[display("it gave {_0}, not {FINGERPRINTS}")]
    Fingerprints(Fingerprints),
    /// Its heights are not 270 × 340 samples.
    #[display("its heights are {columns} × {rows} samples, not 270 × 340")]
    Heights { columns: u32, rows: u32 },
    /// It places other than 142 units of 41 types.
    #[display("it places {units} units of {types} types, not 142 of 41")]
    Units { units: usize, types: usize },
    /// It has other than 438 client objects of 22 looks.
    #[display("it has {objects} client objects of {looks} looks, not 438 of 22")]
    ClientObjects { objects: usize, looks: usize },
    /// It has other than 28 markers.
    #[display("it has {_0} markers, not 28")]
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
    pub(crate) game: Fingerprint,
    pub(crate) rules: Fingerprint,
}
