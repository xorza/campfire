use bevy_math::UVec2;

use crate::error::TextureLevelsError;
use crate::ktx2_image::{IDENTIFIER, LEVEL_ENTRY, LEVEL_INDEX};
use crate::texture_format::TextureFormat;

/// A texture of the package, decoded, as a texture composed of package textures takes it, a
/// terrain's among them (02-engine-core.md, The terrain's texture is composed at load): `size`
/// texels of sRGB red, green, blue and alpha, rows from the top, its first `levels` levels one
/// after another, the largest first. It reads what the import writes of a TGA, a KTX2 file of
/// `R8G8B8A8_SRGB`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureLevels {
    size: UVec2,
    levels: u32,
    data: Vec<u8>,
}

impl TextureLevels {
    /// A texture of `size` texels and its first `levels` levels, `data` their texels, one level
    /// after another, the largest first.
    pub fn new(size: UVec2, levels: u32, data: Vec<u8>) -> Self {
        assert!(
            size.cmpgt(UVec2::ZERO).all() && levels > 0,
            "a texel and a level"
        );
        let texture = Self { size, levels, data };
        let bytes = (0..levels).map(|level| Self::level_len(size, level));
        assert_eq!(
            Some(texture.data.len()),
            bytes.sum::<Option<usize>>(),
            "the bytes of its levels"
        );
        texture
    }

    /// The first `levels` levels of a KTX2 file's `bytes`: a 2D texture of `R8G8B8A8_SRGB`, of no
    /// supercompression, with at least `levels` levels, each of its size's bytes.
    pub fn of_ktx2(bytes: &[u8], levels: u32) -> Result<Self, TextureLevelsError> {
        debug_assert!(levels > 0, "a level wanted");
        if bytes.get(..IDENTIFIER.len()) != Some(&IDENTIFIER[..]) {
            return Err(TextureLevelsError::NotKtx2);
        }
        let word = |at: usize| -> Result<u32, TextureLevelsError> {
            let field = bytes.get(at..at + 4).ok_or(TextureLevelsError::Short)?;
            Ok(u32::from_le_bytes(field.try_into().expect("four bytes")))
        };
        let wide = |at: usize| -> Result<usize, TextureLevelsError> {
            let field = bytes.get(at..at + 8).ok_or(TextureLevelsError::Short)?;
            let value = u64::from_le_bytes(field.try_into().expect("eight bytes"));
            usize::try_from(value).ok().ok_or(TextureLevelsError::Short)
        };
        let format = word(12)?;
        if format != TextureFormat::Rgba8.vk_format() {
            return Err(TextureLevelsError::Format(format));
        }
        let [width, height] = [word(20)?, word(24)?];
        let [depth, layers, faces, held, supercompression] =
            [word(28)?, word(32)?, word(36)?, word(40)?, word(44)?];
        let plain = width > 0 && height > 0 && depth == 0 && layers == 0 && faces == 1;
        if !plain || supercompression != 0 {
            return Err(TextureLevelsError::NotPlain);
        }
        if held < levels {
            return Err(TextureLevelsError::Levels {
                held,
                wanted: levels,
            });
        }
        let mut data = Vec::new();
        for level in 0..levels {
            let entry = LEVEL_INDEX + LEVEL_ENTRY * level as usize;
            let [at, len] = [wide(entry)?, wide(entry + 8)?];
            if Some(len) != Self::level_len(UVec2::new(width, height), level) {
                return Err(TextureLevelsError::LevelBytes(level));
            }
            let end = at.checked_add(len).ok_or(TextureLevelsError::Short)?;
            data.extend_from_slice(bytes.get(at..end).ok_or(TextureLevelsError::Short)?);
        }
        Ok(Self {
            size: UVec2::new(width, height),
            levels,
            data,
        })
    }

    pub const fn size(&self) -> UVec2 {
        self.size
    }

    pub const fn levels(&self) -> u32 {
        self.levels
    }

    /// The texels of level `level`, rows from the top.
    pub fn level(&self, level: u32) -> &[u8] {
        debug_assert!(level < self.levels, "a level it holds");
        let start: usize = (0..level).map(|above| self.level_bytes(above)).sum();
        &self.data[start..start + self.level_bytes(level)]
    }

    fn level_bytes(&self, level: u32) -> usize {
        Self::level_len(self.size, level).expect("a texture's levels fit memory")
    }

    /// The bytes of level `level` of a texture of `size` texels, four to a texel; none past what
    /// a `usize` counts, as an untrusted file's size may name.
    fn level_len(size: UVec2, level: u32) -> Option<usize> {
        let sides = (size >> level).max(UVec2::ONE);
        let texels = u64::from(sides.x).checked_mul(u64::from(sides.y))?;
        usize::try_from(texels.checked_mul(4)?).ok()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_math::UVec2;

    use crate::ktx2_image::Ktx2Image;
    use crate::texture_format::TextureFormat;
    use crate::texture_levels::TextureLevels;

    impl TextureLevels {
        /// A KTX2 file of `size` RGBA8 sRGB texels and `levels` levels, each of `texel` of its
        /// level's number, as the import writes it.
        pub fn ktx2(size: UVec2, levels: u32, texel: fn(u32) -> [u8; 4]) -> Vec<u8> {
            let level = |level: u32| {
                let sides = (size >> level).max(UVec2::ONE);
                texel(level).repeat(sides.element_product() as usize)
            };
            let bytes: Vec<Vec<u8>> = (0..levels).map(level).collect();
            let levels: Vec<&[u8]> = bytes.iter().map(Vec::as_slice).collect();
            Ktx2Image {
                format: TextureFormat::Rgba8,
                width: size.x,
                height: size.y,
                levels: &levels,
            }
            .encode()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktx2_image::Ktx2Image;

    #[test]
    fn a_texture_takes_its_first_levels_the_largest_first() {
        // 8 × 4 texels: levels of 128, 32 and 8 bytes, each of its number.
        let numbered = |level: u32| [u8::try_from(level).unwrap(); 4];
        let texture =
            TextureLevels::of_ktx2(&TextureLevels::ktx2(UVec2::new(8, 4), 4, numbered), 3).unwrap();
        assert_eq!((texture.size(), texture.levels()), (UVec2::new(8, 4), 3));
        let expected: Vec<u8> = [(0, 128), (1, 32), (2, 8)]
            .into_iter()
            .flat_map(|(level, len)| vec![level; len])
            .collect();
        assert_eq!(texture, TextureLevels::new(UVec2::new(8, 4), 3, expected));
        assert_eq!(texture.level(1), [1; 32]);
        assert_eq!(texture.level(2), [2; 8]);
        // Another format, too few levels, a file cut short, or no KTX2 at all, give none.
        let blocks = [16, 8, 8, 8].map(|len| vec![0; len]);
        let blocks: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
        let bc1 = Ktx2Image {
            format: TextureFormat::Bc1,
            width: 8,
            height: 4,
            levels: &blocks,
        };
        let bc1 = TextureLevels::of_ktx2(&bc1.encode(), 3);
        assert_eq!(bc1, Err(TextureLevelsError::Format(134)));
        let two = TextureLevels::of_ktx2(&TextureLevels::ktx2(UVec2::new(8, 4), 2, numbered), 3);
        assert_eq!(two, Err(TextureLevelsError::Levels { held: 2, wanted: 3 }));
        let mut cut = TextureLevels::ktx2(UVec2::new(8, 4), 4, numbered);
        cut.truncate(cut.len() - 1);
        assert_eq!(
            TextureLevels::of_ktx2(&cut, 3),
            Err(TextureLevelsError::Short)
        );
        assert_eq!(
            TextureLevels::of_ktx2(b"no", 3),
            Err(TextureLevelsError::NotKtx2)
        );
    }

    #[test]
    fn every_truncation_of_a_ktx2_file_is_refused_and_no_flip_panics() {
        // A file is untrusted: each byte cut short is refused, and each byte flipped, of its
        // sizes, its index and its texels, is refused or reads levels that hold their bytes.
        let numbered = |level: u32| [u8::try_from(level).unwrap(); 4];
        let bytes = TextureLevels::ktx2(UVec2::new(8, 4), 4, numbered);
        assert!(TextureLevels::of_ktx2(&bytes, 3).is_ok());
        for at in 0..bytes.len() {
            assert!(
                TextureLevels::of_ktx2(&bytes[..at], 3).is_err(),
                "truncated to {at} bytes"
            );
            for flip in [0x01, 0x80, 0xFF] {
                let mut flawed = bytes.clone();
                flawed[at] ^= flip;
                if let Ok(texture) = TextureLevels::of_ktx2(&flawed, 3) {
                    for level in 0..texture.levels() {
                        assert_eq!(
                            Some(texture.level(level).len()),
                            TextureLevels::level_len(texture.size(), level),
                            "byte {at} ^ {flip:#x}"
                        );
                    }
                }
            }
        }
        // The largest sides of a file give a level of 2⁶⁴ − 2³³ + 1 texels, four bytes each, past
        // a `usize`: none, as the decode refuses.
        assert_eq!(TextureLevels::level_len(UVec2::splat(u32::MAX), 0), None);
        assert_eq!(TextureLevels::level_len(UVec2::new(8, 4), 1), Some(32));
    }
}
