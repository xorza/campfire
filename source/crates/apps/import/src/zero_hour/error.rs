use std::path::PathBuf;

use campfire_client_files::{
    EffectsError, FogFileError, TeamColorsError, TerrainFileError, UntestedCondition,
};
use campfire_common::MapName;
use campfire_package::LoadError;
use campfire_package_disk::WriteError;
use campfire_store::PathError;
use campfire_store::ReadError;
use campfire_units::GroundError;
use derive_more::Display;
use thiserror::Error;

use crate::texture::error::TextureError;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::format::big::error::ArchiveError;
use crate::zero_hour::format::csf::error::CsfError;
use crate::zero_hour::format::ini::error::{IniError, MoneyError};
use crate::zero_hour::format::map::error::MapReadError;
use crate::zero_hour::format::w3d::error::W3dError;

/// Why a Zero Hour install does not import: a file of it does not read, it is not one the importer
/// reads, or the package does not write.
#[derive(Debug, Error)]
pub enum ZeroHourError {
    /// A file or a directory of the install does not read.
    #[error("the install does not read")]
    Read(#[from] PathError<ReadError>),
    /// The package does not write.
    #[error("the package does not write")]
    Write(#[from] WriteError),
    /// An archive of the install is no archive the game reads.
    #[error("{} is no archive", .archive.display())]
    Archive {
        archive: PathBuf,
        #[source]
        error: ArchiveError,
    },
    /// An archive's length or time of change differs from the ones its entries were read with:
    /// another program changed it during the import.
    #[error("{} changed during the import", .archive.display())]
    ArchiveChanged { archive: PathBuf },
    /// Two archives of the install are one to the game, whose paths differ only in case: which
    /// one it reads is not known, and a check of one would pass the other.
    #[error("archives {first} and {second} are one archive to the game")]
    ArchiveClash { first: String, second: String },
    /// The install's archives are no version the importer knows: the first archive that differs
    /// from the version they were compared with, and how.
    #[error("no version the importer knows: {archive} {difference}")]
    UnknownVersion {
        archive: String,
        difference: VersionDifference,
    },
    /// No archive of the install holds a file the import reads.
    #[error("no archive holds {0}")]
    NotInArchives(ArchivePath),
    /// A map of the install is no map the game reads, by its path.
    #[error("{path} is no map the game reads")]
    Map {
        path: ArchivePath,
        #[source]
        error: MapError,
    },
    /// A texture of the install is none the importer converts, by its path.
    #[error("{path} is no texture the importer converts")]
    Texture {
        path: ArchivePath,
        #[source]
        error: TextureError,
    },
    /// The game data gives a camera no view takes: a height not above 0, or a value not finite.
    #[error("the game data gives a camera no view takes")]
    Camera,
    /// The package the import wrote does not load on one of its maps, as a match would load it.
    #[error("the package does not load on its map {map}")]
    Load {
        map: MapName,
        #[source]
        error: Box<LoadError>,
    },
    /// An INI file is none the game reads, by its path.
    #[error("{path} is no INI file the game reads")]
    Ini {
        path: ArchivePath,
        #[source]
        error: IniError,
    },
    /// A W3D file of the install is none the importer reads, by its path.
    #[error("{path} is no W3D file the importer reads")]
    W3d {
        path: ArchivePath,
        #[source]
        error: W3dError,
    },
    /// A model the objects name does not convert, by its name.
    #[error("model {model} does not convert")]
    Model {
        model: String,
        #[source]
        error: ModelError,
    },
    /// `GameData.ini`'s damage thresholds do not fall from 1 to above 0, so no life share holds
    /// them.
    #[error("the game data's damage thresholds are no shares of life")]
    DamageThresholds,
    /// A value of retaliation of `AIData.ini` is no number the engine holds.
    #[error("the AI data's retaliation is no number the engine holds")]
    Retaliation,
    /// A veterancy bonus of `GameData.ini` is no number the engine holds.
    #[error("the game data's veterancy bonus is no number the engine holds")]
    VeterancyBonus,
    /// The garrison's bonus of `GameData.ini` is no number the engine holds.
    #[error("the game data's garrison bonus is no number the engine holds")]
    GarrisonBonus,
    /// A propaganda tower's radius or heal, or the game data's bonus of its rate of fire, is no
    /// number the engine holds.
    #[error("a propaganda tower's numbers are no numbers the engine holds")]
    Propaganda,
    /// The general's ranks of `Rank.ini` are not numbered from 1 on, rank 1 needs skill points,
    /// or a rank needs no more than the one before.
    #[error("the general's ranks do not rise from rank 1 at no skill points")]
    Ranks,
    /// A science a template or a science names, by its name, is no science of `Science.ini`.
    #[error("science {0} is no science of the INI files")]
    NoScience(String),
    /// A science, by its name, needs a rank past the last.
    #[error("science {0} needs a rank past the last")]
    ScienceRank(String),
    /// A state holds a number the engine does not hold: a stride, a speed or a rise that is not
    /// finite, or a speed range not above 0 and in order.
    #[error("a state holds a stride, a speed or a rise the engine does not hold")]
    StateNumber,
    /// A draw module's state names a condition flag that is no declared name.
    #[error("a draw module names the condition flag {0}, which is no name")]
    ConditionFlag(String),
    /// Several language directories hold art, and the importer cannot tell which the game reads.
    #[error("several language directories hold art: {0:?}")]
    Languages(Vec<String>),
    #[error(transparent)]
    Name(#[from] NameError),
    /// A look the import made names a condition of no test, which the client refuses to read.
    #[error(transparent)]
    Untested(#[from] UntestedCondition),
    /// A particle system or an effect list holds a number the client refuses, one not finite.
    #[error(transparent)]
    Effects(#[from] EffectsError),
    /// `GameData.ini`'s shroud levels give a clear cell less light than a fogged one, or a fogged
    /// one less than a shrouded one.
    #[error(transparent)]
    Fog(#[from] FogFileError),
    /// A player template a player plays, by its name, names no starting building.
    #[error("player template {0} names no starting building")]
    NoStart(String),
    /// No player template is one a player plays.
    #[error("no player template is one a player plays")]
    NoFaction,
    /// No language directory holds art, so the import cannot tell which string table the game
    /// reads.
    #[error("no language directory holds art, so no string table is the game's")]
    NoLanguage,
    /// The multiplayer settings' choices of starting money make no option of the mode.
    #[error("the multiplayer settings' starting money")]
    Money(#[from] MoneyError),
    /// Two armors' names give one id in the mode.
    #[error("the armor {0} has a name another armor's gives")]
    ArmorName(String),
    /// An armor's share of a damage type is below 0, or past what the engine holds.
    #[error("an armor's share is past what the engine holds: {0}")]
    ArmorShare(String),
    /// An object names an armor no file defines by the name of its case, which the game's debug
    /// build stops at as wrong data.
    #[error("{object} names the armor {armor}, which no file defines")]
    NoArmor { object: String, armor: String },
    /// An upgrade's name gives the modifier's name another upgrade's gives, or an effect's
    /// modifier takes the name of an upgrade's or of a weapon stage's.
    #[error("the upgrade or effect {0} has a name another modifier's gives")]
    UpgradeName(String),
    /// A weapon's name makes the action's name another weapon's of other numbers makes.
    #[error("the weapon {0} has a name no action may take once")]
    WeaponName(String),
    /// The string table is none the game reads, by its path.
    #[error("{path} is no string table the game reads")]
    StringTable {
        path: ArchivePath,
        #[source]
        error: CsfError,
    },
    /// A start object of the player template, by its name, is no object of the INI files.
    #[error("start object {0} is no object of the INI files")]
    NoStartObject(String),
    /// A start object of the player template gives no unit type.
    #[error("start object {object}")]
    StartType {
        object: String,
        #[source]
        error: TypeError,
    },
    /// Two maps' directories give one map name.
    #[error("{first} and {second} give one map name")]
    MapNameClash {
        first: ArchivePath,
        second: ArchivePath,
    },
    /// Two textures that the models or the terrain draw with go to one file of the package, as
    /// an `x.dds` and an `x.tga` would.
    #[error("{first} and {second} go to one texture file")]
    TextureClash {
        first: ArchivePath,
        second: ArchivePath,
    },
    #[error(transparent)]
    NoPackagePath(#[from] NoPackagePath),
    /// A number of the wreck of an object or a creation list of this name is not finite, or past
    /// what a number holds.
    #[error("a number of the wreck of {name} is not finite, or past what a number holds")]
    Wreck { name: String },
    /// A spring of the chassis of an object of this name is not finite, or past what a number
    /// holds.
    #[error("a spring of the chassis of {name} is not finite, or past what a number holds")]
    Chassis { name: String },
    /// The width of an object's track is not finite, or past what a number holds.
    #[error("the width of the track of {name} is not finite, or past what a number holds")]
    Track { name: String },
    /// The game's `Multiplayer.ini` gives no team colors the engine takes.
    #[error("the game's team colors are none the engine takes")]
    TeamColors(#[from] TeamColorsError),
}

/// Why a time of the game is none the import writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum TimeError {
    /// It is below 0.
    #[error("the time is below 0")]
    Negative,
    /// It is below 0, not finite, or of more frames than the game's `Int` holds.
    #[error("the time is of no count of frames the game's Int holds")]
    PastInt,
}

/// Why an object gives no unit type.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TypeError {
    #[error(transparent)]
    Name(#[from] NameError),
    /// It names a locomotor no file defines, which the game refuses.
    #[error("it names the locomotor {0}, which no file defines")]
    NoLocomotor(String),
    /// A speed, a turn rate or a radius of it is not finite, or past what the engine holds.
    #[error("a number of it is past what the engine holds")]
    Number,
    /// Its body is a circle or a box no unit type of the engine may have.
    #[error("its body is none the engine holds")]
    Body,
    /// Its turret turns at a rate no unit type of the engine may have.
    #[error("its turret is none the engine holds")]
    Turret,
    /// Its slow death keeps it for no time the game or the engine holds.
    #[error("its slow death's delay is no time the engine holds")]
    DeathDelay,
    /// It hovers by a lift no greater than the game's gravity, or to no height, so it never
    /// climbs, or at a number the engine's flight, or its wings, do not hold.
    #[error("its flight is none the engine holds")]
    Flight,
    /// It drives by a locomotor of a number the engine's mover does not hold, or the game's
    /// threshold of a really damaged body is no share.
    #[error("its locomotor is none the engine's mover holds")]
    Locomotor,
    /// It is a jet of no taxi locomotor that drives, or of a takeoff pause or an idle time the
    /// engine does not hold.
    #[error("its sortie is none the engine holds")]
    Sortie,
}

/// How an install's archive differs from a version's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum VersionDifference {
    /// The install has an archive the version has not.
    #[display("is not in it")]
    Extra,
    /// The install lacks an archive of the version.
    #[display("is missing")]
    Missing,
    /// The archive holds other bytes than the version's.
    #[display("holds other bytes")]
    OtherBytes,
}

/// Why a map is none the importer turns into a package's: its bytes do not read, or what they
/// read converts to no map of the engine.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapError {
    #[error(transparent)]
    Read(#[from] MapReadError),
    /// Its heights are no grid within the world's bound.
    #[error("its heights are no grid within the world's bound")]
    Heights,
    /// Its ground's coarse cells, the original's partition over its playable area, make no ground
    /// the engine loads.
    #[error("its playable area makes no ground of its heights")]
    Ground(#[source] GroundError),
    /// Its heights and the points its objects stand at make no bounds within the world's bound.
    #[error("its heights and its objects' points make no bounds within the world's bound")]
    Bounds,
    /// A waypoint's name, which a marker's param keeps, is not ASCII.
    #[error("a waypoint's name is not ASCII")]
    NotAscii,
    /// The terrain converts to no terrain file: a cliff's coordinates are not finite.
    #[error("its terrain converts to no terrain file")]
    TerrainFile(#[source] TerrainFileError),
    /// An object, by its index among those the game keeps, lies at no position or angle the
    /// engine holds.
    #[error("object {0} lies at no position or angle the engine holds")]
    Placement(usize),
    /// A waypoint's id is negative.
    #[error("waypoint id {0} is negative")]
    WaypointId(i32),
    /// Two waypoints share an id.
    #[error("two waypoints have id {0}")]
    WaypointTwice(i32),
    /// A waypoint's name reads as a number, which a marker's param would take for one.
    #[error("waypoint name {0} reads as a number")]
    WaypointName(String),
    /// A light it draws its objects in goes no way.
    #[error("a light it draws its objects in goes no way")]
    LightDirection,
    #[error(transparent)]
    Name(#[from] NameError),
    /// An object it places gives no unit type.
    #[error("object {object}")]
    UnitType {
        object: String,
        #[source]
        error: TypeError,
    },
}

/// Why a name of the original gives none in the package.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NameError {
    #[error(transparent)]
    Clash(#[from] NameClash),
    /// As the package names it, it passes the bytes a declared name holds,
    /// `DeclaredName::MAX_LEN`.
    #[error("{0} passes the bytes a name holds")]
    TooLong(String),
    /// The projectile of a weapon of `object` gives the name `name` of the object's own unit
    /// type.
    #[error("a projectile of {object} gives {name}, the name of its own unit type")]
    ProjectileOfType { object: String, name: String },
}

/// Two objects of the original whose names give one name in the package, a look's or a unit
/// type's: the one that took it first, and the one refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{first} and {second} give one name")]
pub struct NameClash {
    pub first: String,
    pub second: String,
}

/// A path the import made of the game's names that no package path spells, as a name of `:`,
/// `..` or a device's name gives.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0} is no path a package can name")]
pub struct NoPackagePath(pub String);

/// Why a model the objects name does not convert.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModelError {
    /// Its HLOD names a hierarchy no file holds.
    #[error("its hierarchy {0} is in no file")]
    NoHierarchy(String),
    /// A sub-object or a skin's vertex names a pivot its hierarchy lacks.
    #[error("a pivot its hierarchy lacks")]
    Pivot,
    /// A skin is drawn alone, with no hierarchy to place it.
    #[error("a skin with no hierarchy")]
    SkinAlone,
    /// A mesh's vertex colors go to a light the glTF cannot color by them, as its vertex
    /// materials give emissive light, or no diffuse light.
    #[error("vertex colors of no diffuse light")]
    VertexColors,
    /// A mesh it draws has no triangle.
    #[error("mesh {0} has no triangle")]
    Empty(String),
    /// A bone a unit's turret or weapon reads lies past the bound of the engine's frame of a
    /// unit.
    #[error("a bone of a turret or a weapon lies past the engine's bound")]
    Bound,
    /// Two nodes have one name, so a default look cannot hide one of them alone.
    #[error("two nodes are named {0}")]
    NodeName(String),
    /// A shader blends by factors no shipped shader uses.
    #[error("a shader blends by source {src} and destination {dest}")]
    Shader { src: u8, dest: u8 },
    /// A shader mixes its detail stage by a color function no shipped shader uses.
    #[error("a shader mixes its detail stage by function {0}")]
    DetailFunction(u8),
    #[error(transparent)]
    NoPackagePath(#[from] NoPackagePath),
}
