use crate::zero_hour::error::ClassTextureError;

/// A terrain class's texture as the atlas takes it: `width` by `height` texels of sRGB red,
/// green, blue and alpha, rows from the top, its first three levels one after another, the
/// largest first: what the import writes of a TGA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassTexture {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
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
/// The levels the atlas takes.
const LEVELS: usize = 3;

impl ClassTexture {
    /// The texture of a KTX2 file's `bytes`: a 2D texture of `R8G8B8A8_SRGB`, of no
    /// supercompression, with at least three levels, each of its size's bytes.
    pub fn of_ktx2(bytes: &[u8]) -> Result<ClassTexture, ClassTextureError> {
        if bytes.get(..IDENTIFIER.len()) != Some(&IDENTIFIER[..]) {
            return Err(ClassTextureError::NotKtx2);
        }
        let word = |at: usize| -> Result<u32, ClassTextureError> {
            let field = bytes.get(at..at + 4).ok_or(ClassTextureError::Short)?;
            Ok(u32::from_le_bytes(field.try_into().expect("four bytes")))
        };
        let wide = |at: usize| -> Result<usize, ClassTextureError> {
            let field = bytes.get(at..at + 8).ok_or(ClassTextureError::Short)?;
            let value = u64::from_le_bytes(field.try_into().expect("eight bytes"));
            usize::try_from(value).ok().ok_or(ClassTextureError::Short)
        };
        let format = word(12)?;
        if format != RGBA8_SRGB {
            return Err(ClassTextureError::Format(format));
        }
        let [width, height] = [word(20)?, word(24)?];
        let [depth, layers, faces, levels, supercompression] =
            [word(28)?, word(32)?, word(36)?, word(40)?, word(44)?];
        if depth != 0 || layers != 0 || faces != 1 || supercompression != 0 {
            return Err(ClassTextureError::NotPlain);
        }
        if (levels as usize) < LEVELS {
            return Err(ClassTextureError::Levels(levels));
        }
        let mut data = Vec::new();
        for level in 0..LEVELS {
            let entry = LEVEL_INDEX + LEVEL_ENTRY * level;
            let [at, len] = [wide(entry)?, wide(entry + 8)?];
            let sides = (width >> level).max(1) as usize * (height >> level).max(1) as usize;
            if len != sides * 4 {
                return Err(ClassTextureError::LevelBytes(level));
            }
            let end = at.checked_add(len).ok_or(ClassTextureError::Short)?;
            data.extend_from_slice(bytes.get(at..end).ok_or(ClassTextureError::Short)?);
        }
        Ok(ClassTexture {
            width,
            height,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A KTX2 file of `width` by `height` RGBA8 sRGB texels and `levels` levels, each texel its
    /// level's number, the levels stored the smallest first, as the import writes them.
    fn ktx2(width: u32, height: u32, levels: u32, format: u32) -> Vec<u8> {
        let mut out = IDENTIFIER.to_vec();
        for value in [
            format, 1, width, height, 0, 0, 1, levels, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ] {
            out.extend(value.to_le_bytes());
        }
        let level_bytes =
            |level: u32| ((width >> level).max(1) * (height >> level).max(1) * 4) as usize;
        let mut at = LEVEL_INDEX + LEVEL_ENTRY * levels as usize;
        let mut places = vec![0; levels as usize];
        for level in (0..levels).rev() {
            places[level as usize] = at;
            at += level_bytes(level);
        }
        for level in 0..levels {
            let len = level_bytes(level) as u64;
            for value in [places[level as usize] as u64, len, len] {
                out.extend(value.to_le_bytes());
            }
        }
        for level in (0..levels).rev() {
            out.extend(vec![u8::try_from(level).unwrap(); level_bytes(level)]);
        }
        out
    }

    #[test]
    fn a_class_texture_takes_its_first_three_levels_the_largest_first() {
        // 8 × 4 texels: levels of 128, 32 and 8 bytes, each of its number.
        let texture = ClassTexture::of_ktx2(&ktx2(8, 4, 4, RGBA8_SRGB)).unwrap();
        assert_eq!((texture.width, texture.height), (8, 4));
        let expected: Vec<u8> = [(0, 128), (1, 32), (2, 8)]
            .into_iter()
            .flat_map(|(level, len)| vec![level; len])
            .collect();
        assert_eq!(texture.data, expected);
        // Another format, two levels, a file cut short, or no KTX2 at all, give none.
        let bc1 = ClassTexture::of_ktx2(&ktx2(8, 4, 4, 134));
        assert_eq!(bc1, Err(ClassTextureError::Format(134)));
        let two = ClassTexture::of_ktx2(&ktx2(8, 4, 2, RGBA8_SRGB));
        assert_eq!(two, Err(ClassTextureError::Levels(2)));
        let mut cut = ktx2(8, 4, 4, RGBA8_SRGB);
        cut.truncate(cut.len() - 1);
        assert_eq!(ClassTexture::of_ktx2(&cut), Err(ClassTextureError::Short));
        assert_eq!(
            ClassTexture::of_ktx2(b"no"),
            Err(ClassTextureError::NotKtx2)
        );
    }
}
