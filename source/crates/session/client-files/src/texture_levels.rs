use bevy_math::UVec2;

use crate::error::TextureLevelsError;

/// A texture of the package, decoded, as a texture composed of package textures takes it, a
/// terrain's among them (design 16, D2): `size` texels of sRGB red, green, blue and alpha, rows
/// from the top, its first `levels` levels one after another, the largest first. It reads what
/// the import writes of a TGA, a KTX2 file of `R8G8B8A8_SRGB`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureLevels {
    size: UVec2,
    levels: u32,
    data: Vec<u8>,
}

/// KTX2's identifier: `«KTX 20»\r\n\x1A\n`.
const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
/// Vulkan's `R8G8B8A8_SRGB`.
const RGBA8_SRGB: u32 = 43;
/// The bytes before the level index, and a level's entry there.
const LEVEL_INDEX: usize = 80;
const LEVEL_ENTRY: usize = 24;

impl TextureLevels {
    /// A texture of `size` texels and its first `levels` levels, `data` their texels, one level
    /// after another, the largest first.
    pub fn new(size: UVec2, levels: u32, data: Vec<u8>) -> TextureLevels {
        assert!(
            size.cmpgt(UVec2::ZERO).all() && levels > 0,
            "a texel and a level"
        );
        let texture = TextureLevels { size, levels, data };
        assert_eq!(
            texture.data.len(),
            (0..levels)
                .map(|level| texture.level_bytes(level))
                .sum::<usize>(),
            "the bytes of its levels"
        );
        texture
    }

    /// The first `levels` levels of a KTX2 file's `bytes`: a 2D texture of `R8G8B8A8_SRGB`, of no
    /// supercompression, with at least `levels` levels, each of its size's bytes.
    pub fn of_ktx2(bytes: &[u8], levels: u32) -> Result<TextureLevels, TextureLevelsError> {
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
        if format != RGBA8_SRGB {
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
            let sides = (width >> level).max(1) as usize * (height >> level).max(1) as usize;
            if len != sides * 4 {
                return Err(TextureLevelsError::LevelBytes(level));
            }
            let end = at.checked_add(len).ok_or(TextureLevelsError::Short)?;
            data.extend_from_slice(bytes.get(at..end).ok_or(TextureLevelsError::Short)?);
        }
        Ok(TextureLevels {
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
        (self.size >> level).max(UVec2::ONE).element_product() as usize * 4
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_math::UVec2;

    use crate::texture_levels::{IDENTIFIER, LEVEL_ENTRY, LEVEL_INDEX, RGBA8_SRGB, TextureLevels};

    impl TextureLevels {
        /// A KTX2 file of `size` RGBA8 sRGB texels and `levels` levels, each of `texel` of its
        /// level's number, the levels stored the smallest first, as the import writes them.
        pub fn ktx2(size: UVec2, levels: u32, texel: fn(u32) -> [u8; 4]) -> Vec<u8> {
            TextureLevels::ktx2_of(size, levels, RGBA8_SRGB, texel)
        }

        pub(crate) fn ktx2_of(
            size: UVec2,
            levels: u32,
            format: u32,
            texel: fn(u32) -> [u8; 4],
        ) -> Vec<u8> {
            let mut out = IDENTIFIER.to_vec();
            for value in [
                format, 1, size.x, size.y, 0, 0, 1, levels, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ] {
                out.extend(value.to_le_bytes());
            }
            let texels = |level: u32| (size >> level).max(UVec2::ONE).element_product() as usize;
            let mut at = LEVEL_INDEX + LEVEL_ENTRY * levels as usize;
            let mut places = vec![0; levels as usize];
            for level in (0..levels).rev() {
                places[level as usize] = at;
                at += texels(level) * 4;
            }
            for level in 0..levels {
                let len = (texels(level) * 4) as u64;
                for value in [places[level as usize] as u64, len, len] {
                    out.extend(value.to_le_bytes());
                }
            }
            for level in (0..levels).rev() {
                out.extend(texel(level).repeat(texels(level)));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let bc1 = TextureLevels::of_ktx2(
            &TextureLevels::ktx2_of(UVec2::new(8, 4), 4, 134, numbered),
            3,
        );
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
}
