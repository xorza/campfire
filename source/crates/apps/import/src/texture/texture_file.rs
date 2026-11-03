use bevy_math::UVec2;

use crate::texture::dds_file::DdsFile;
use crate::texture::error::TextureError;
use crate::texture::texture_kind::TextureKind;
use crate::texture::tga_file::{TgaAlpha, TgaFile};

/// A texture file read once, of its kind, which gives its size and its KTX2.
#[derive(Debug)]
pub(crate) enum TextureFile<'a> {
    Dds(DdsFile<'a>),
    Tga(TgaFile),
}

impl<'a> TextureFile<'a> {
    /// The texture `bytes` hold, of `kind`, a TGA's alpha as `alpha` takes it.
    pub(crate) fn read(
        kind: TextureKind,
        bytes: &'a [u8],
        alpha: TgaAlpha,
    ) -> Result<Self, TextureError> {
        Ok(match kind {
            TextureKind::Dds => Self::Dds(DdsFile::read(bytes)?),
            TextureKind::Tga => Self::Tga(TgaFile::read(bytes, alpha)?),
        })
    }

    pub(crate) const fn size(&self) -> UVec2 {
        match self {
            Self::Dds(file) => file.size(),
            Self::Tga(file) => file.size(),
        }
    }

    /// Its KTX2 file, as each kind writes it.
    pub(crate) fn ktx2(self) -> Vec<u8> {
        match self {
            Self::Dds(file) => file.ktx2(),
            Self::Tga(file) => file.ktx2(),
        }
    }
}
