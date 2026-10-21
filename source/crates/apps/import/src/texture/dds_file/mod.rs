use campfire_client_files::{Ktx2Image, TextureFormat};

use crate::texture::error::TextureError;

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
    pub(crate) fn read(bytes: &'a [u8]) -> Result<Self, TextureError> {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        if bytes.len() < 4 + HEADER
            || &bytes[..4] != MAGIC
            || usize::try_from(word(4)) != Ok(HEADER)
        {
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
pub(crate) mod samples {
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
mod tests;
