use thiserror::Error;

/// Why a texture file is none the importer converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TextureError {
    /// A DDS does not start with `DDS ` and a header of 124 bytes.
    #[error("it is no DDS")]
    NotDds,
    /// A DDS's pixel format is no DXT1, DXT3 or DXT5.
    #[error("its pixel format {0:?} is no DXT1, DXT3 or DXT5")]
    DdsFormat([u8; 4]),
    /// A DDS is a cube map or a volume.
    #[error("it is a cube map or a volume")]
    NotFlat,
    /// A block texture's sides are no whole blocks of 4 texels, or a texture has no texel.
    #[error("its {width} × {height} texels fill no whole blocks")]
    Size { width: u32, height: u32 },
    /// A DDS holds more levels than its full chain.
    #[error("it holds {0} levels, past its full chain")]
    Levels(u32),
    /// A file's bytes past its header are not its texels' length.
    #[error("it holds {found} bytes of texels, not {expected}")]
    Length { expected: usize, found: usize },
    /// A TGA is not uncompressed true color with no color map.
    #[error("it is TGA image type {0} or has a color map")]
    TgaType(u8),
    /// A TGA's texels are not 24 or 32 bits.
    #[error("its texels are {0} bits, not 24 or 32")]
    TgaDepth(u8),
}
