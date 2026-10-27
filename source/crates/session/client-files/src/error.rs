use std::num::NonZeroU8;
use std::str::Utf8Error;

use campfire_codec::{BinaryError, TomlError};
use campfire_common::DeclaredName;
use thiserror::Error;

/// Why a client file of a package does not read.
#[derive(Debug, Error)]
pub enum ClientFileError {
    /// A TOML file's bytes are not UTF-8.
    #[error("its text is not UTF-8")]
    Text(#[from] Utf8Error),
    #[error("{0}")]
    Toml(#[from] TomlError),
    #[error("{0}")]
    Binary(#[from] BinaryError),
}

/// A draw of a unit's look names this condition, which the looks give no test, so no client could
/// tell whether a unit holds it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("a look's state names the condition `{0}`, of no test")]
pub struct UntestedCondition(pub DeclaredName);

/// Why an effects file is none the client plays (02-engine-core.md, Effects are a format of the
/// engine).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EffectsError {
    /// Its step is not above 0, or not finite.
    #[error("its step is not a length of time")]
    Step,
    /// Its wrecks' gravity pulls up, a share of their ground is past 0 to 1, or a number is not
    /// finite.
    #[error("its wrecks' ground does not hold")]
    Wrecks,
    /// Its treads' speed or a scale of their dust is below 0, or a number is not finite.
    #[error("its treads' dust does not hold")]
    Treads,
    /// Its tracks' spacing or fade is not above 0, they hold fewer than two edges or more opaque
    /// than all, or a number is not finite.
    #[error("its tracks do not hold")]
    Tracks,
    /// The system of this name holds a number not finite, or keys out of order, or none.
    #[error("the particle system `{0}` holds a number not finite or keys out of order")]
    System(DeclaredName),
    /// An entry of the list of this name holds a number not finite.
    #[error("an entry of the effect `{0}` holds a number not finite")]
    List(DeclaredName),
    /// A list or a system names a system of this name, which the file does not hold.
    #[error("the particle system `{0}` is named and not held")]
    Unknown(DeclaredName),
    /// The sound of this name holds no file, a volume, a share or a pitch past its bounds, or a
    /// range whose silence is not past its full volume.
    #[error("the sound `{0}` does not hold")]
    Sound(DeclaredName),
    /// A list names a sound of this name, which the file does not hold.
    #[error("the sound `{0}` is named and not held")]
    UnknownSound(DeclaredName),
}

/// Why a sound file is none the client plays (02-engine-core.md, The listener is the game's
/// microphone; 02-engine-core.md, Music by a choice).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SoundFileError {
    /// A class's volume is past 0 to 1.
    #[error("a volume is past 0 to 1")]
    Volume,
    /// The listener's height is below 0 or not finite, or its share past 0 to 1.
    #[error("the listener's place does not hold")]
    Listener,
    /// The zoom's far is not past its near, its near below 0, or its boost past 0 to 1.
    #[error("the zoom's boost does not hold")]
    Zoom,
    /// The playlist of this value holds no track.
    #[error("the playlist of `{0}` holds no track")]
    Playlist(DeclaredName),
}

/// Why a fog file is none the client draws (02-engine-core.md, The fog is the one material's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FogFileError {
    /// A seen cell keeps less light than an explored one, or an explored one than a shrouded one.
    #[error("a seen cell is darker than an explored one, or an explored one than a shrouded one")]
    Order,
}

/// Why a terrain file is none the client draws (02-engine-core.md, Client): a shape, an index, a
/// number or a rectangle past what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TerrainFileError {
    /// Its grid has fewer than two columns, or its ground quads fill no whole rows of its cells.
    #[error("its ground quads fill no whole rows of its cells")]
    Shape,
    /// An overlay, by its index, names a cell past the ground's.
    #[error("overlay {0} names a cell past the ground's")]
    Cell(usize),
    /// A texture coordinate is not finite.
    #[error("a texture coordinate is not finite")]
    Coordinate,
    /// A decal's point is not finite.
    #[error("a decal's point is not finite")]
    Point,
    /// A decal triangle, by its index, names a vertex past the decals'.
    #[error("decal triangle {0} names a vertex past the decals'")]
    Triangle(usize),
    /// A side of its texture is past `TerrainTexture::MOST_SIDE`.
    #[error("a side of its texture is past the longest a terrain texture has")]
    TextureSize,
    /// Its texture has no texel, or no level, or more levels than its sides halve to.
    #[error("its texture has no texel or no level, or more levels than its sides hold")]
    Texture,
    /// A copy, by its index, lies past the texture, or its place or size lies off the texels its
    /// last level halves to.
    #[error("copy {0} lies past the texture or off its levels' texels")]
    Copy(usize),
    /// A border wrap, by its index, lies past the texture, is wider than its rectangle, or lies
    /// off the texels its last level halves to.
    #[error("border wrap {0} lies past the texture, is too wide, or lies off its levels' texels")]
    Wrap(usize),
    /// A factor of its surface is not from 0 to 1.
    #[error("a factor of its surface is not from 0 to 1")]
    Surface,
}

/// Why a texture of the package is none a composed texture takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TextureLevelsError {
    /// It starts with no KTX2 identifier.
    #[error("it is no KTX2 file")]
    NotKtx2,
    /// A field or a level runs past its end.
    #[error("it runs past its end")]
    Short,
    /// Its format, by its Vulkan number, is none of `R8G8B8A8_SRGB` and BC1 to BC3's sRGB.
    #[error("its format {0} is none of R8G8B8A8_SRGB and BC1 to BC3's sRGB")]
    Format(u32),
    /// It is no 2D texture of texels, one face and no layers, or its data is supercompressed.
    #[error("it is no plain 2D texture")]
    NotPlain,
    /// It holds fewer levels than are wanted of it.
    #[error("it has {held} levels, fewer than {wanted}")]
    Levels { held: u32, wanted: u32 },
    /// A level, by its number, is not its size's bytes.
    #[error("level {0} is not its size's bytes")]
    LevelBytes(u32),
}

/// Why a list of colors is no team colors: a mode's team colors file, or what an import writes as
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TeamColorsError {
    /// It gives no color.
    #[error("it gives no color")]
    None,
    /// It gives more colors, its count, than `TeamColors::MOST`.
    #[error("it gives {0} colors, more than 256")]
    TooMany(usize),
}

/// A place of the command panel given two buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the panel's place {0} holds two buttons")]
pub struct CrowdedPlace(pub NonZeroU8);
