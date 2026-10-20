use std::fmt;

use derive_more::Display;
use miniz_oxide::inflate::TINFLStatus;
use thiserror::Error;

use crate::zero_hour::format::error::CutShort;
use crate::zero_hour::format::ref_pack::error::RefPackError;

/// Why a map's bytes are no map the game reads.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapReadError {
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
    /// Its time of day is none of the game's four.
    #[error("its time of day {0} is none of the game's four")]
    TimeOfDay(i32),
    /// A number of its lighting is not finite.
    #[error("a number of its lighting is not finite")]
    Lighting,
}

/// Why a map's terrain is not one the import converts: a count or an index that reaches past
/// what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TerrainError {
    /// Its cells fill no whole rows of its columns.
    #[error("its cells fill no whole rows")]
    Shape,
    /// A cell, a blend or a cliff's UVs name a tile past its tiles.
    #[error("tile {0} is past its tiles")]
    Tile(u16),
    /// A cell names a blend past its blends.
    #[error("blend {0} is past its blends")]
    Blend(u16),
    /// A cell names cliff UVs past its list.
    #[error("cliff UVs {0} are past its list")]
    CliffUv(u16),
    /// A texture class, by its index, reaches past its tiles.
    #[error("texture class {0} reaches past its tiles")]
    Class(usize),
    /// A texture class, by its index, is more tiles a side than the atlas's grid holds.
    #[error("texture class {0} is wider than the atlas")]
    ClassWidth(usize),
    /// An edge class, by its index, reaches past its edge tiles.
    #[error("edge class {0} reaches past its edge tiles")]
    EdgeClassTiles(usize),
    /// A blend names an edge class past its edge classes.
    #[error("edge class {0} is past its edge classes")]
    EdgeClass(u16),
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
    GlobalLighting,
    WorldInfo,
}

impl Chunk {
    /// Its name in a map's table.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::HeightMapData => "HeightMapData",
            Self::BlendTileData => "BlendTileData",
            Self::ObjectsList => "ObjectsList",
            Self::Object => "Object",
            Self::GlobalLighting => "GlobalLighting",
            Self::WorldInfo => "WorldInfo",
        }
    }
}

impl fmt::Display for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl From<CutShort> for MapReadError {
    fn from(_: CutShort) -> Self {
        Self::Short
    }
}
