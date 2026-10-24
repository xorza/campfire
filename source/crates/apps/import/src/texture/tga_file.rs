use bevy_math::UVec2;
use campfire_client_files::{Ktx2Image, TextureFormat};

use crate::texture::error::TextureError;
use crate::texture::mip_chain::MipChain;

/// An uncompressed true-color TGA (Truevision's TGA 2.0), its texels as 8-bit red, green, blue
/// and alpha, rows from the top; a texel of 24 bits is opaque, and one of 32 has the alpha its
/// reader's `TgaAlpha` takes.
#[derive(Debug)]
pub(crate) struct TgaFile {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Which a texel of 32 bits takes as its alpha: its fourth byte as TGA 2.0 says, only when the
/// header counts alpha bits, or that byte whatever the header counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TgaAlpha {
    /// The header's count of alpha bits, its descriptor's low four, says: a texel is opaque when
    /// it counts none.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "TGA 2.0's own rule, which the general reader holds and no imported game reads by yet"
        )
    )]
    Counted,
    /// The fourth byte, as a loader that never reads the count takes it.
    Always,
}

/// The header of a TGA the importer reads, checked against the bytes after it: its size, the
/// bytes of a texel, where its texels start, and its descriptor byte.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TgaHeader {
    width: u32,
    height: u32,
    texel: usize,
    start: usize,
    descriptor: u8,
}

/// The header's bytes, and the footer of TGA 2.0 that may follow the texels.
const HEADER: usize = 18;
const FOOTER: usize = 26;
const SIGNATURE: &[u8; 18] = b"TRUEVISION-XFILE.\0";

/// Uncompressed true color, the image type the importer reads.
const TRUE_COLOR: u8 = 2;

impl TgaHeader {
    /// The header of the TGA `bytes` hold, refused unless it is uncompressed true color of 24 or
    /// 32 bits with no color map, and its texels end the file or a footer that names no extension
    /// area follows them.
    pub(crate) fn read(bytes: &[u8]) -> Result<Self, TextureError> {
        if bytes.len() < HEADER {
            return Err(TextureError::Length {
                expected: HEADER,
                found: bytes.len(),
            });
        }
        let (id, color_map, image_type) = (bytes[0], bytes[1], bytes[2]);
        if color_map != 0 || image_type != TRUE_COLOR {
            return Err(TextureError::TgaType(image_type));
        }
        let half = |at: usize| u32::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
        let (width, height, depth, descriptor) = (half(12), half(14), bytes[16], bytes[17]);
        let texel = match depth {
            24 => 3,
            32 => 4,
            _ => return Err(TextureError::TgaDepth(depth)),
        };
        if width == 0 || height == 0 {
            return Err(TextureError::Size { width, height });
        }
        let header = Self {
            width,
            height,
            texel,
            start: HEADER + usize::from(id),
            descriptor,
        };
        let expected = header.texels_len();
        let after = bytes.len().saturating_sub(header.start);
        let footer = bytes.len() >= FOOTER
            && bytes.ends_with(SIGNATURE)
            && bytes[bytes.len() - FOOTER..][..8] == [0; 8];
        if after != expected && !(footer && after == expected + FOOTER) {
            return Err(TextureError::Length {
                expected,
                found: after,
            });
        }
        Ok(header)
    }

    /// Its width and height, in texels.
    pub(crate) const fn size(&self) -> UVec2 {
        UVec2::new(self.width, self.height)
    }

    /// The length of its texels as stored.
    fn texels_len(&self) -> usize {
        usize::try_from(self.width).expect("a u32 fits usize")
            * usize::try_from(self.height).expect("a u32 fits usize")
            * self.texel
    }
}

impl TgaFile {
    /// The TGA `bytes` hold, its alpha as `alpha` takes it, refused as `TgaHeader::read` refuses
    /// its header.
    pub(crate) fn read(bytes: &[u8], alpha: TgaAlpha) -> Result<Self, TextureError> {
        let header = TgaHeader::read(bytes)?;
        let TgaHeader {
            width,
            height,
            texel,
            start,
            descriptor,
        } = header;
        let columns = usize::try_from(width).expect("a u32 fits usize");
        let rows = usize::try_from(height).expect("a u32 fits usize");
        let expected = header.texels_len();
        let texels = &bytes[start..start + expected];
        // Bit 5 of the descriptor puts the first row at the top, bit 4 the first column at the
        // right.
        let (from_top, from_right) = (descriptor & 0x20 != 0, descriptor & 0x10 != 0);
        let alpha_byte = match alpha {
            TgaAlpha::Counted => descriptor & 0x0F != 0,
            TgaAlpha::Always => true,
        };
        let mut rgba = Vec::with_capacity(columns * rows * 4);
        for row in 0..rows {
            let stored = if from_top { row } else { rows - 1 - row };
            for column in 0..columns {
                let stored_column = if from_right {
                    columns - 1 - column
                } else {
                    column
                };
                let at = (stored * columns + stored_column) * texel;
                let bgra = &texels[at..at + texel];
                rgba.extend([
                    bgra[2],
                    bgra[1],
                    bgra[0],
                    bgra.get(3)
                        .copied()
                        .filter(|_| alpha_byte)
                        .unwrap_or(u8::MAX),
                ]);
            }
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Its KTX2 file: 8-bit RGBA, with its full chain of levels.
    pub(crate) fn ktx2(self) -> Vec<u8> {
        let chain = MipChain::of(self.width, self.height, self.rgba);
        let levels: Vec<&[u8]> = chain.levels.iter().map(Vec::as_slice).collect();
        Ktx2Image {
            format: TextureFormat::Rgba8,
            width: self.width,
            height: self.height,
            levels: &levels,
        }
        .encode()
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// A TGA of `width` by `height` texels of `depth` bits, `descriptor` its last header byte,
    /// `texels` as stored, and the TGA 2.0 footer that names no extension area.
    pub(crate) fn tga(
        width: u16,
        height: u16,
        depth: u8,
        descriptor: u8,
        texels: &[u8],
    ) -> Vec<u8> {
        let mut header = [0_u8; 18];
        header[2] = 2;
        header[12..14].copy_from_slice(&width.to_le_bytes());
        header[14..16].copy_from_slice(&height.to_le_bytes());
        header[16] = depth;
        header[17] = descriptor;
        [&header[..], texels, &[0; 8], b"TRUEVISION-XFILE.\0"].concat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::tga_file::samples::tga;

    fn words(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    #[test]
    fn a_tga_becomes_a_ktx2_of_rgba_from_the_top_with_its_levels() {
        // 4 × 2, 32 bits, its bottom row first, each texel blue, green, red, alpha: black, white
        // and grey 128 of alpha 100, as `MipChain`'s test lays them out from the top.
        let (b, w, g) = ([0, 0, 0, 0], [255, 255, 255, 255], [128, 128, 128, 100]);
        let file = tga(4, 2, 32, 0, &[w, b, g, g, b, w, g, g].concat());
        assert_eq!(TgaHeader::read(&file).unwrap().size(), UVec2::new(4, 2));
        let ktx2 = TgaFile::read(&file, TgaAlpha::Always).unwrap().ktx2();
        // The header counts no alpha bits: read by the count, each texel is opaque; read always,
        // its fourth byte. Counted 8 bits, the count reads that byte too.
        let alphas = |descriptor: u8, alpha: TgaAlpha| {
            let read = TgaFile::read(&tga(1, 1, 32, descriptor, &g), alpha).unwrap();
            read.rgba[3]
        };
        assert_eq!(alphas(0, TgaAlpha::Counted), 255);
        assert_eq!(alphas(0, TgaAlpha::Always), 100);
        assert_eq!(alphas(8, TgaAlpha::Counted), 100);
        // Levels 4 × 2, 2 × 1 and 1 × 1, of 32, 8 and 4 bytes. The level index ends at 80 + 3 · 24
        // = 152; the descriptor, 4 + 24 + 4 · 16 = 92 bytes, ends at 244; levels align to 4: 1 × 1
        // at 244, 2 × 1 at 248, the base at 256, to 288.
        let expected = [
            vec![
                0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
            ],
            // VK_FORMAT_R8G8B8A8_SRGB.
            words(&[43, 1, 4, 2, 0, 0, 1, 3, 0]),
            words(&[152, 92, 0, 0, 0, 0, 0, 0]),
            words(&[256, 0, 32, 0, 32, 0]),
            words(&[248, 0, 8, 0, 8, 0]),
            words(&[244, 0, 4, 0, 4, 0]),
            // RGBSDA, BT.709, sRGB; blocks of 1 texel, 4 bytes. Red, green and blue of 8 bits at
            // 0, 8 and 16; alpha, channel 15, linear, at 24; each from 0 to 255.
            words(&[92, 0, 2 + (88 << 16), 1 + (1 << 8) + (2 << 16), 0, 4, 0]),
            words(&[7 << 16, 0, 0, 255]),
            words(&[8 + (7 << 16) + (1 << 24), 0, 0, 255]),
            words(&[16 + (7 << 16) + (2 << 24), 0, 0, 255]),
            words(&[24 + (7 << 16) + (0x1F << 24), 0, 0, 255]),
            // The 1 × 1 level from the light of the level above, as `MipChain`'s test works it
            // out: 161, where the codes above would give 162.
            vec![161, 161, 161, 114],
            vec![188, 188, 188, 128, 128, 128, 128, 100],
            [b, w, g, g, w, b, g, g].concat(),
        ]
        .concat();
        assert_eq!(ktx2, expected);
    }

    #[test]
    fn a_tga_reads_its_channels_and_rows_as_its_header_says() {
        // One texel of 24 bits, blue 1, green 2, red 3: opaque.
        let one = TgaFile::read(&tga(1, 1, 24, 0, &[1, 2, 3]), TgaAlpha::Counted).unwrap();
        assert_eq!(one.rgba, [3, 2, 1, 255]);
        // 2 × 2, each texel's red its place in the file; the first row at the top and the first
        // column at the right, by the descriptor's bits.
        let texels: Vec<u8> = (0..4).flat_map(|at| [0, 0, at]).collect();
        let red = |descriptor| -> Vec<u8> {
            let read =
                TgaFile::read(&tga(2, 2, 24, descriptor, &texels), TgaAlpha::Counted).unwrap();
            read.rgba.chunks(4).map(|texel| texel[0]).collect()
        };
        assert_eq!(red(0x00), [2, 3, 0, 1]);
        assert_eq!(red(0x20), [0, 1, 2, 3]);
        assert_eq!(red(0x10), [3, 2, 1, 0]);
        assert_eq!(red(0x30), [1, 0, 3, 2]);
        // A color map, another image type, 16 bits, no texels, and texels short or followed by
        // bytes that are no footer.
        let mut mapped = tga(1, 1, 24, 0, &[1, 2, 3]);
        mapped[1] = 1;
        assert_eq!(
            TgaFile::read(&mapped, TgaAlpha::Counted).map(|_| ()),
            Err(TextureError::TgaType(2))
        );
        let mut rle = tga(1, 1, 24, 0, &[1, 2, 3]);
        rle[2] = 10;
        assert_eq!(
            TgaFile::read(&rle, TgaAlpha::Counted).map(|_| ()),
            Err(TextureError::TgaType(10))
        );
        assert_eq!(
            TgaFile::read(&tga(1, 1, 16, 0, &[1, 2]), TgaAlpha::Counted).map(|_| ()),
            Err(TextureError::TgaDepth(16))
        );
        assert_eq!(
            TgaFile::read(&tga(0, 1, 24, 0, &[]), TgaAlpha::Counted).map(|_| ()),
            Err(TextureError::Size {
                width: 0,
                height: 1
            })
        );
        let bare = &tga(1, 1, 24, 0, &[1, 2, 3])[..21];
        assert!(TgaFile::read(bare, TgaAlpha::Counted).is_ok());
        assert_eq!(
            TgaFile::read(&bare[..20], TgaAlpha::Counted).map(|_| ()),
            Err(TextureError::Length {
                expected: 3,
                found: 2
            })
        );
        assert_eq!(
            TgaFile::read(&[bare, &[9]].concat(), TgaAlpha::Counted).map(|_| ()),
            Err(TextureError::Length {
                expected: 3,
                found: 4
            })
        );
    }
}
