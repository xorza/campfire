//! Textures into KTX2, as any import writes them: a DDS's DXT blocks as they are, and a TGA's
//! texels as 8-bit RGBA with levels computed in linear light, every one sRGB.

pub(crate) mod dds_file;
pub(crate) mod error;
pub(crate) mod ktx2_image;
pub(crate) mod mip_chain;
pub(crate) mod srgb;
pub(crate) mod texture_format;
pub(crate) mod tga_file;

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::texture::dds_file::DdsFile;
    use crate::texture::dds_file::internals::dds;
    use crate::texture::tga_file::TgaFile;
    use crate::texture::tga_file::internals::tga;

    /// A DXT1 texture of 8 × 8 texels and its full chain, and a TGA of 2 × 1.
    pub(crate) fn textures() -> [Vec<u8>; 2] {
        [
            dds(*b"DXT1", 8, 8, 4, &[7; 32 + 8 + 8 + 8]),
            tga(2, 1, 32, 0, &[1, 2, 3, 4, 5, 6, 7, 8]),
        ]
    }

    /// The two textures as the import writes them, KTX2.
    pub(crate) fn ktx2() -> [Vec<u8>; 2] {
        let [rock, sign] = textures();
        [
            DdsFile::read(&rock).expect("a fixture DDS reads").ktx2(),
            TgaFile::read(&sign).expect("a fixture TGA reads").ktx2(),
        ]
    }
}
