//! Textures into KTX2, as any import writes them: a DDS's DXT blocks as they are, and a TGA's
//! texels as 8-bit RGBA with levels computed in linear light, every one sRGB.

pub(crate) mod dds_file;
pub(crate) mod error;
pub(crate) mod mip_chain;
pub(crate) mod srgb;
pub(crate) mod tga_file;

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use campfire_store::internals::PinnedFile;

    use crate::texture::internals::ktx2;

    #[test]
    fn the_clients_fixtures_are_the_textures_the_import_writes() {
        // The client's test of Bevy's KTX2 loader reads them, as it depends on no import: a DDS
        // of DXT1 blocks, 8 × 8 with its 4 levels, BC1; and a 32-bit TGA of 2 × 1, RGBA8 with its
        // 2 levels. `CAMPFIRE_BLESS=1` writes them again.
        let fixtures = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../client/tests/fixtures"
        ));
        for (name, bytes) in ["dxt1.ktx2", "rgba.ktx2"].into_iter().zip(ktx2()) {
            PinnedFile::new(&fixtures.join(name)).pin(&bytes, 1 << 10);
        }
    }
}
