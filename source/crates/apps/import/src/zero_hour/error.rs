use std::path::PathBuf;

use campfire_client_files::TerrainFileError;
use campfire_package_disk::WriteError;
use campfire_store::PathError;
use campfire_store::ReadError;
use derive_more::Display;
use thiserror::Error;

use crate::texture::error::TextureError;
use crate::zero_hour::format::big::error::ArchiveError;
use crate::zero_hour::format::csf::error::CsfError;
use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::map::error::MapReadError;
use crate::zero_hour::format::map::error::TerrainError;
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
    NotInArchives(String),
    /// A map of the install is no map the game reads, by its path.
    #[error("{map} is no map the game reads")]
    Map {
        map: String,
        #[source]
        error: MapError,
    },
    /// A texture of the install is none the importer converts, by its path.
    #[error("{path} is no texture the importer converts")]
    Texture {
        path: String,
        #[source]
        error: TextureError,
    },
    /// The game data gives a camera no view takes: a height not above 0, or a value not finite.
    #[error("the game data gives a camera no view takes")]
    Camera,
    /// An INI file is none the game reads, by its path.
    #[error("{path} is no INI file the game reads")]
    Ini {
        path: String,
        #[source]
        error: IniError,
    },
    /// A W3D file of the install is none the importer reads, by its path.
    #[error("{path} is no W3D file the importer reads")]
    W3d {
        path: String,
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
    /// Several language folders hold art, and the importer cannot tell which the game reads.
    #[error("several language folders hold art: {0:?}")]
    Languages(Vec<String>),
    #[error(transparent)]
    NameClash(#[from] NameClash),
    /// The player template a skirmish starts with, by its name, is none the install defines, or
    /// names no starting building.
    #[error("player template {0} names no starting building")]
    NoStart(String),
    /// No language folder holds art, so the import cannot tell which string table the game reads.
    #[error("no language folder holds art, so no string table is the game's")]
    NoLanguage,
    /// The multiplayer settings mark no choice of starting money as the default, or several.
    #[error("the multiplayer settings mark no single default starting money")]
    DefaultMoney,
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
    /// A weapon's name makes the action's name another weapon's of other numbers makes.
    #[error("the weapon {0} has a name no action may take once")]
    WeaponName(String),
    /// The string table is none the game reads, by its path.
    #[error("{path} is no string table the game reads")]
    StringTable {
        path: String,
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
    /// Two maps' folders give one map name.
    #[error("{first} and {second} give one map name")]
    MapNameClash { first: String, second: String },
    #[error(transparent)]
    NoPackagePath(#[from] NoPackagePath),
}

/// Why an object gives no unit type.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TypeError {
    #[error(transparent)]
    NameClash(#[from] NameClash),
    /// It names a locomotor no file defines, which the game refuses.
    #[error("it names the locomotor {0}, which no file defines")]
    NoLocomotor(String),
    /// A speed, a turn rate or a radius of it is not finite, or past what the engine holds.
    #[error("a number of it is past what the engine holds")]
    Number,
    /// Its body is a circle or a box no unit type of the engine may have.
    #[error("its body is none the engine holds")]
    Body,
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
    /// Its heights and the points its objects stand at make no bounds within the world's bound.
    #[error("its heights and its objects' points make no bounds within the world's bound")]
    Bounds,
    /// A waypoint's name, which a marker's param keeps, is not ASCII.
    #[error("a waypoint's name is not ASCII")]
    NotAscii,
    /// The terrain names past what it holds.
    #[error("its terrain names past what it holds")]
    Terrain(#[source] TerrainError),
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
    NameClash(#[from] NameClash),
    /// An object it places gives no unit type.
    #[error("object {object}")]
    UnitType {
        object: String,
        #[source]
        error: TypeError,
    },
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
