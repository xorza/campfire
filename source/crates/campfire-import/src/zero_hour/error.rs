use std::fmt;
use std::path::PathBuf;

use campfire_package::TerrainError;
use derive_more::Display;
use miniz_oxide::inflate::TINFLStatus;
use thiserror::Error;

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
