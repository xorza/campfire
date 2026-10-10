use std::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::path::PathBuf;

use campfire_package::TerrainError;
use derive_more::Display;
use miniz_oxide::inflate::TINFLStatus;
use thiserror::Error;

use crate::texture::error::TextureError;

/// Why a Zero Hour install is not one the importer reads.
#[derive(Debug, Error)]
pub enum ZeroHourError {
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
    /// Two objects' names give one unit type name.
    #[error("{first} and {second} give one unit type name")]
    UnitTypeClash { first: String, second: String },
    /// Two maps' folders give one map name.
    #[error("{first} and {second} give one map name")]
    MapNameClash { first: String, second: String },
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

/// Why an archive's bytes are no `.big` archive.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchiveError {
    /// It does not start with `BIGF`.
    #[error("it does not start with BIGF")]
    NotBig,
    /// Its header, or its list of entries, ends past the file or its own length.
    #[error("its header ends past its length")]
    Short,
    /// An entry's bytes lie past the file's end.
    #[error("{0}'s bytes lie past its end")]
    EntryPastEnd(String),
    /// An entry's path is not ASCII.
    #[error("an entry's path is not ASCII")]
    NotAscii,
}

/// Why a `RefPack` stream does not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RefPackError {
    /// It ends inside its header, a command or its literal bytes.
    #[error("it ends inside a command")]
    Short,
    /// A copy reaches back before the first byte written.
    #[error("a copy reaches before the start")]
    BeforeStart,
    /// It writes more bytes than its header gives.
    #[error("it writes more than its {0} bytes")]
    TooLong(usize),
    /// It ends having written fewer bytes than its header gives.
    #[error("it writes {written} of its {len} bytes")]
    TooShort { written: usize, len: usize },
}

/// Why a map's bytes are no map the game reads, or none the importer turns into a package's.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum MapError {
    /// It is packed in a form the game reads but no shipped map uses.
    #[error("it is packed as {0}, which the importer does not read")]
    Packing(Packing),
    /// Its `RefPack` stream does not decode.
    #[error("its RefPack stream does not decode")]
    RefPack(#[source] RefPackError),
    /// Its zlib stream does not inflate, or inflates past its header's size.
    #[error("its zlib stream does not inflate: {0:?}")]
    Zlib(TINFLStatus),
    /// It unpacks to another size than its header gives.
    #[error("it unpacks to {written} bytes, not its header's {len}")]
    PackedSize { written: usize, len: usize },
    /// It does not start with `CkMp`.
    #[error("it does not start with CkMp")]
    NotChunks,
    /// It ends inside its table, a chunk or a field.
    #[error("it ends inside a chunk")]
    Short,
    /// Its table names an id twice.
    #[error("its table names id {0} twice")]
    NameTwice(u32),
    /// A chunk or a dictionary key names an id its table lacks.
    #[error("it names id {0}, which its table lacks")]
    UnknownName(u32),
    /// It lacks a chunk the import reads.
    #[error("it holds no {0} chunk")]
    Missing(Chunk),
    /// It holds a chunk twice.
    #[error("it holds a second {0} chunk")]
    Twice(Chunk),
    /// A chunk is of a version no shipped map has.
    #[error("its {chunk} chunk is version {version}, which the importer does not read")]
    Version { chunk: Chunk, version: u16 },
    /// A chunk holds bytes past its last field.
    #[error("its {0} chunk holds bytes past its fields")]
    Leftover(Chunk),
    /// A count is negative.
    #[error("a count of {0} is negative")]
    Negative(i32),
    /// Its heights or its terrain hold other than one entry for each sample.
    #[error("it holds {len} entries for {width} × {depth} samples")]
    Samples {
        len: usize,
        width: usize,
        depth: usize,
    },
    /// Its heights are no grid within the world's bound.
    #[error("its heights are no grid within the world's bound")]
    Heights,
    /// A dictionary entry is of a type the game does not read.
    #[error("a dictionary entry is of type {0}")]
    DictType(u8),
    /// A blended tile, by its index in the file, lacks the mark that ends it.
    #[error("blended tile {0} lacks its end mark")]
    BlendMark(usize),
    /// A blended tile sets other than one direction.
    #[error("blended tile {0} sets other than one direction")]
    BlendShape(usize),
    /// A blended tile sets inverted bits the game does not read.
    #[error("blended tile {0} sets unknown inverted bits")]
    BlendInverted(usize),
    /// A tile, blend, cliff or edge index is negative or past its list.
    #[error("index {0} is past its list")]
    Index(i32),
    /// A text the import keeps is not ASCII.
    #[error("a name is not ASCII")]
    NotAscii,
    /// The terrain names past what it holds.
    #[error("its terrain names past what it holds")]
    Terrain(#[source] TerrainError),
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
    /// Two object templates give one unit type name.
    #[error("{first} and {second} give one unit type name")]
    UnitTypeClash { first: String, second: String },
}

/// A packing the game reads that the importer does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum Packing {
    #[display("NOX")]
    Nox,
    #[display("EAB")]
    BTree,
    #[display("EAH")]
    Huffman,
}

/// A chunk of a map that the import reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chunk {
    HeightMapData,
    BlendTileData,
    ObjectsList,
    Object,
}

impl Chunk {
    /// Its name in a map's table.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Chunk::HeightMapData => "HeightMapData",
            Chunk::BlendTileData => "BlendTileData",
            Chunk::ObjectsList => "ObjectsList",
            Chunk::Object => "Object",
        }
    }
}

impl fmt::Display for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Why an INI file is none the game reads, each with its line from 1.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IniError {
    /// An `End` closes no block.
    #[error("line {line}: an End closes no block")]
    StrayEnd { line: usize },
    /// The file ends inside a block.
    #[error("it ends inside a block")]
    Unclosed,
    /// A line outside every block starts none of the file's blocks: `Object` or `ObjectReskin`
    /// in an object file, `Terrain` in a terrain file, `GameData` in a game data file.
    #[error("line {line}: no block of the file starts here")]
    NotBlock { line: usize },
    /// A block lacks its name, or an `ObjectReskin` its base's.
    #[error("line {line}: a block lacks a name")]
    NoName { line: usize },
    /// A field the importer reads has no value.
    #[error("line {line}: a field has no value")]
    NoValue { line: usize },
    /// A field of yes or no holds neither.
    #[error("line {line}: a field holds neither Yes nor No")]
    Bool { line: usize },
    /// A field of a real number holds none.
    #[error("line {line}: a field holds no number")]
    Real {
        line: usize,
        #[source]
        error: ParseFloatError,
    },
    /// A field of a whole number holds none.
    #[error("line {line}: a field holds no whole number")]
    Number {
        line: usize,
        #[source]
        error: ParseIntError,
    },
    /// A model draw module's first state is neither `DefaultConditionState` nor `NONE`.
    #[error("line {line}: a draw module's first state is no default")]
    NoDefaultState { line: usize },
    /// A `DefaultConditionState` follows another state.
    #[error("line {line}: a DefaultConditionState is not the first state")]
    LateDefaultState { line: usize },
    /// A model draw module's default state names no `Model`.
    #[error("line {line}: a default state names no model")]
    NoModel { line: usize },
    /// A weapon bone names no slot of `PRIMARY`, `SECONDARY` and `TERTIARY`, or no bone.
    #[error("line {line}: a weapon bone names no slot and bone")]
    WeaponSlot { line: usize },
}

/// Why a W3D file's bytes are no model the importer converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum W3dError {
    /// A chunk or a field runs past its end.
    #[error("a chunk runs past its end")]
    Short,
    /// A name is not ASCII.
    #[error("a name is not ASCII")]
    NotAscii,
    /// A chunk lacks a part it needs, as a mesh its header.
    #[error("a chunk lacks a part")]
    Missing,
    /// An index passes its list, or a pivot's parent does not come before it.
    #[error("an index passes its list")]
    Index,
    /// A mesh's lists are not its header's counts.
    #[error("a mesh's lists are not its counts")]
    Counts,
    /// A float is infinite or not a number.
    #[error("a float is not finite")]
    NotFinite,
    /// An HLOD has other than one level of detail.
    #[error("an HLOD has {0} levels of detail")]
    Levels(u32),
    /// A mesh is of a geometry the importer does not draw, as a camera-aligned one.
    #[error("a mesh is of geometry {0:#x}")]
    Geometry(u32),
}

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
}
