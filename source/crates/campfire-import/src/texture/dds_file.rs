use crate::texture::error::TextureError;
use crate::texture::ktx2_image::Ktx2Image;
use crate::texture::texture_format::TextureFormat;

/// A DDS of DXT blocks (Microsoft's `DDS_HEADER`): its format, its base size and its levels'
/// blocks, as the file holds them, which a KTX2 file of the same block format takes as they are.
#[derive(Debug)]
pub(crate) struct DdsFile<'a> {
    format: TextureFormat,
    width: u32,
    height: u32,
    levels: Vec<&'a [u8]>,
}

/// `DDS ` and the header's size, 124.
const MAGIC: &[u8; 4] = b"DDS ";
const HEADER: usize = 124;

/// The header flag that makes its mip count count.
const MIPMAPCOUNT: u32 = 0x2_0000;

impl<'a> DdsFile<'a> {
    /// The DDS `bytes` hold, refused unless its levels are whole blocks of DXT1, DXT3 or DXT5, no
    /// more than its full chain, and its bytes end with them.
    pub(crate) fn read(bytes: &'a [u8]) -> Result<DdsFile<'a>, TextureError> {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        if bytes.len() < 4 + HEADER || &bytes[..4] != MAGIC || word(4) as usize != HEADER {
            return Err(TextureError::NotDds);
        }
        let (flags, height, width, count) = (word(8), word(12), word(16), word(28));
        let four_cc: [u8; 4] = bytes[84..88].try_into().expect("4 bytes");
        let format = match &four_cc {
            b"DXT1" => TextureFormat::Bc1,
            b"DXT3" => TextureFormat::Bc2,
            b"DXT5" => TextureFormat::Bc3,
            _ => return Err(TextureError::DdsFormat(four_cc)),
        };
        if word(112) != 0 {
            return Err(TextureError::NotFlat);
        }
        if width == 0 || height == 0 || width % 4 != 0 || height % 4 != 0 {
            return Err(TextureError::Size { width, height });
        }
        let count = if flags & MIPMAPCOUNT != 0 && count > 0 {
            count
        } else {
            1
        };
        let full = 32 - width.max(height).leading_zeros();
        if count > full {
            return Err(TextureError::Levels(count));
        }
        // A header's sides may reach 2³², so each level's bytes, and their sum, are checked.
        let sides = |level: u32| ((width >> level).max(1), (height >> level).max(1));
        let lengths = (0..count)
            .map(|level| {
                let (x, y) = sides(level);
                format.level_bytes(x, y)
            })
            .collect::<Option<Vec<usize>>>();
        let expected = lengths
            .as_ref()
            .and_then(|lengths| {
                lengths
                    .iter()
                    .try_fold(0_usize, |sum, &len| sum.checked_add(len))
            })
            .ok_or(TextureError::Size { width, height })?;
        let mut rest = &bytes[4 + HEADER..];
        if rest.len() != expected {
            return Err(TextureError::Length {
                expected,
                found: rest.len(),
            });
        }
        let mut levels = Vec::new();
        for len in lengths.into_iter().flatten() {
            let (level, after) = rest.split_at(len);
            levels.push(level);
            rest = after;
        }
        Ok(DdsFile {
            format,
            width,
            height,
            levels,
        })
    }

    /// Its KTX2 file: the same blocks and levels.
    pub(crate) fn ktx2(&self) -> Vec<u8> {
        Ktx2Image {
            format: self.format,
            width: self.width,
            height: self.height,
            levels: &self.levels,
        }
        .encode()
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// A DDS of `four_cc` blocks, `width` by `height`, of `levels` mip levels whose bytes follow,
    /// with the mip count flag set.
    pub(crate) fn dds(
        four_cc: [u8; 4],
        width: u32,
        height: u32,
        levels: u32,
        data: &[u8],
    ) -> Vec<u8> {
        let mut header = vec![0_u8; 124];
        let mut put =
            |at: usize, value: u32| header[at - 4..at].copy_from_slice(&value.to_le_bytes());
        put(4, 124);
        put(8, 0x1 | 0x2 | 0x4 | 0x1000 | 0x2_0000);
        put(12, height);
        put(16, width);
        put(28, levels);
        put(76, 32);
        put(80, 0x4);
        header[80..84].copy_from_slice(&four_cc);
        [&b"DDS "[..], &header, data].concat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::dds_file::internals::dds;

    /// The little-endian bytes of `words`.
    fn words(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    #[test]
    fn a_dds_becomes_a_ktx2_of_its_blocks_as_they_are() {
        // DXT1, 8 × 8 and its full chain: 2 × 2 blocks of 8 bytes, then one block each for 4 × 4,
        // 2 × 2 and 1 × 1.
        let levels: [Vec<u8>; 4] = [
            (0..32).collect(),
            (100..108).collect(),
            (110..118).collect(),
            (120..128).collect(),
        ];
        let file = dds(*b"DXT1", 8, 8, 4, &levels.concat());
        let ktx2 = DdsFile::read(&file).unwrap().ktx2();
        // The level index ends at 80 + 4 · 24 = 176, where the descriptor starts: its 4 bytes of
        // size and a block of 24 and one sample of 16, 44 bytes, so it ends at 220. Levels align
        // to BC1's 8 bytes, from 224, the smallest first: 1 × 1 at 224, 2 × 2 at 232, 4 × 4 at
        // 240, and the base at 248, to 280.
        let expected = [
            vec![
                0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
            ],
            // VK_FORMAT_BC1_RGBA_SRGB_BLOCK, type size 1, 8 × 8, depth 0, no layers, one face,
            // four levels, no supercompression.
            words(&[134, 1, 8, 8, 0, 0, 1, 4, 0]),
            // The descriptor at 176 of 44 bytes; no key/value data; no global data, two 64-bit 0.
            words(&[176, 44, 0, 0, 0, 0, 0, 0]),
            // Each level's offset, length and unpacked length, 64-bit each, from the base.
            words(&[248, 0, 32, 0, 32, 0]),
            words(&[240, 0, 8, 0, 8, 0]),
            words(&[232, 0, 8, 0, 8, 0]),
            words(&[224, 0, 8, 0, 8, 0]),
            // Size 44; Khronos' basic block; version 2 and block size 40; BC1A, BT.709, sRGB,
            // straight alpha; blocks of 4 × 4; 8 bytes a block. The sample: bits 0 to 63 of
            // channel 1, alpha present, at position 0, from 0 to all ones.
            words(&[
                44,
                0,
                2 + (40 << 16),
                128 + (1 << 8) + (2 << 16),
                3 + (3 << 8),
                8,
                0,
            ]),
            words(&[(63 << 16) + (1 << 24), 0, 0, u32::MAX]),
            vec![0; 4],
            levels[3].clone(),
            levels[2].clone(),
            levels[1].clone(),
            levels[0].clone(),
        ]
        .concat();
        assert_eq!(ktx2, expected);
    }

    #[test]
    fn a_dds_the_importer_cannot_convert_is_refused() {
        let blocks = vec![0; 32 + 8 + 8 + 8];
        let read = |bytes: &[u8]| DdsFile::read(bytes).map(|_| ());
        assert_eq!(
            read(&dds(*b"DXT2", 8, 8, 4, &blocks)),
            Err(TextureError::DdsFormat(*b"DXT2"))
        );
        assert_eq!(
            read(&dds(*b"DXT5", 8, 8, 4, &blocks)),
            Err(TextureError::Length {
                expected: 112,
                found: 56
            })
        );
        assert_eq!(
            read(&dds(*b"DXT1", 6, 8, 1, &blocks)),
            Err(TextureError::Size {
                width: 6,
                height: 8
            })
        );
        assert_eq!(
            read(&dds(*b"DXT1", 8, 8, 5, &blocks)),
            Err(TextureError::Levels(5))
        );
        assert_eq!(
            read(&[&dds(*b"DXT1", 8, 8, 4, &blocks)[..], &[0]].concat()),
            Err(TextureError::Length {
                expected: 56,
                found: 57
            })
        );
        assert_eq!(read(b"DDS "), Err(TextureError::NotDds));
        // Sides near 2³², in DXT5 blocks of 16 bytes: the base level alone is under 2⁶⁴ bytes,
        // but its full chain of 32 levels sums past what `usize` holds.
        let huge = dds(*b"DXT5", 0xFFFF_FFFC, 0xFFFF_FFFC, 32, &blocks);
        assert_eq!(
            read(&huge),
            Err(TextureError::Size {
                width: 0xFFFF_FFFC,
                height: 0xFFFF_FFFC
            })
        );
        let mut cube = dds(*b"DXT1", 8, 8, 4, &blocks);
        cube[112] = 0x00;
        cube[113] = 0x02;
        assert_eq!(read(&cube), Err(TextureError::NotFlat));
        // With no mip count flag, one level: the base's 32 bytes alone.
        let mut one = dds(*b"DXT1", 8, 8, 4, &blocks[..32]);
        one[10] = 0;
        assert!(read(&one).is_ok());
    }
}
