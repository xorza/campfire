use bevy_math::UVec2;

/// A texture's format in a package, every one sRGB-encoded color: the block formats a DDS's DXT
/// blocks are, or 8-bit RGBA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// DXT1: 4 × 4 texels in 8 bytes, with punch-through alpha.
    Bc1,
    /// DXT3: 4 × 4 texels in 16 bytes, explicit 4-bit alpha before the color.
    Bc2,
    /// DXT5: 4 × 4 texels in 16 bytes, interpolated alpha before the color.
    Bc3,
    Rgba8,
}

/// A sample of a format's data format descriptor: its first bit, its count of bits, its channel
/// with its qualifiers, and the top of its range.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sample {
    pub(crate) offset: u16,
    pub(crate) bits: u8,
    pub(crate) channel: u8,
    pub(crate) upper: u32,
}

/// The qualifier of a sample that is linear in an sRGB format: alpha.
const LINEAR: u8 = 0x10;

/// The four colors a BC block's color half gives, each of sRGB red, green, blue and alpha.
type BlockColors = [[u8; 4]; 4];

impl TextureFormat {
    const ALL: [Self; 4] = [Self::Bc1, Self::Bc2, Self::Bc3, Self::Rgba8];

    /// The format of Vulkan's number `format`; none for one no package texture is.
    pub(crate) fn of_vk_format(format: u32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|known| known.vk_format() == format)
    }

    /// Vulkan's number for it, `VK_FORMAT_BC1_RGBA_SRGB_BLOCK` and the rest.
    pub(crate) const fn vk_format(self) -> u32 {
        match self {
            Self::Bc1 => 134,
            Self::Bc2 => 136,
            Self::Bc3 => 138,
            Self::Rgba8 => 43,
        }
    }

    /// The side of a block, in texels.
    pub(crate) const fn block_side(self) -> u32 {
        match self {
            Self::Bc1 | Self::Bc2 | Self::Bc3 => 4,
            Self::Rgba8 => 1,
        }
    }

    /// The bytes of a block.
    pub(crate) const fn block_bytes(self) -> u32 {
        match self {
            Self::Bc1 => 8,
            Self::Bc2 | Self::Bc3 => 16,
            Self::Rgba8 => 4,
        }
    }

    /// The bytes of a level of `width` by `height` texels: its blocks, a part block counted
    /// whole; `None` past what `usize` holds.
    pub const fn level_bytes(self, width: u32, height: u32) -> Option<usize> {
        let side = self.block_side();
        let columns = width.div_ceil(side) as usize;
        let rows = height.div_ceil(side) as usize;
        match columns.checked_mul(rows) {
            Some(blocks) => blocks.checked_mul(self.block_bytes() as usize),
            None => None,
        }
    }

    /// The Khronos data format model: `RGBSDA`, or the block model of its compression.
    pub(crate) const fn model(self) -> u8 {
        match self {
            Self::Bc1 => 128,
            Self::Bc2 => 129,
            Self::Bc3 => 130,
            Self::Rgba8 => 1,
        }
    }

    /// Its samples, as Khronos' descriptor of the Vulkan format lists them: a BC1 block's one,
    /// with alpha present; a BC2 or BC3 block's alpha then color; 8-bit red, green, blue and
    /// alpha. Alpha is linear.
    pub(crate) const fn samples(self) -> &'static [Sample] {
        const BLOCK: u32 = u32::MAX;
        match self {
            Self::Bc1 => &[Sample {
                offset: 0,
                bits: 64,
                channel: 1,
                upper: BLOCK,
            }],
            Self::Bc2 | Self::Bc3 => &[
                Sample {
                    offset: 0,
                    bits: 64,
                    channel: 15 | LINEAR,
                    upper: BLOCK,
                },
                Sample {
                    offset: 64,
                    bits: 64,
                    channel: 0,
                    upper: BLOCK,
                },
            ],
            Self::Rgba8 => &[
                Sample {
                    offset: 0,
                    bits: 8,
                    channel: 0,
                    upper: 255,
                },
                Sample {
                    offset: 8,
                    bits: 8,
                    channel: 1,
                    upper: 255,
                },
                Sample {
                    offset: 16,
                    bits: 8,
                    channel: 2,
                    upper: 255,
                },
                Sample {
                    offset: 24,
                    bits: 8,
                    channel: 15 | LINEAR,
                    upper: 255,
                },
            ],
        }
    }
}

impl TextureFormat {
    /// Appends to `texels` a level of `size` texels of this format whose data is `bytes`, its
    /// `level_bytes`, as sRGB red, green, blue and alpha, rows from the top. A block decodes as
    /// Vulkan's specification gives it (Khronos Data Format 1.3, S3TC): its 5:6:5 endpoints
    /// widened by repeating their high bits, the colors between them each rounded to nearest; a
    /// BC1 block of its first endpoint at most its second gives their mean and a transparent
    /// black, a BC2's and a BC3's color half always the four colors; a BC2's alpha is 4 bits
    /// widened, a BC3's two endpoints and the six or four values between them, rounded to
    /// nearest, then 0 and 255. A block past the level's edge is cut to it.
    pub(crate) fn decode(self, size: UVec2, bytes: &[u8], texels: &mut Vec<u8>) {
        debug_assert_eq!(
            Some(bytes.len()),
            self.level_bytes(size.x, size.y),
            "a level's bytes"
        );
        if self == Self::Rgba8 {
            texels.extend_from_slice(bytes);
            return;
        }
        let [width, height] = size.to_array().map(|side| side as usize);
        let start = texels.len();
        texels.resize(start + width * height * 4, 0);
        let level = &mut texels[start..];
        let columns = width.div_ceil(4);
        let block_bytes = self.block_bytes() as usize;
        for (at, block) in bytes.chunks_exact(block_bytes).enumerate() {
            let [x, y] = [at % columns * 4, at / columns * 4];
            let mut texel = [[0; 4]; 16];
            match self {
                Self::Bc1 => self.color_half(block, &mut texel),
                Self::Bc2 => {
                    self.color_half(&block[8..], &mut texel);
                    let alpha = u64::from_le_bytes(block[..8].try_into().expect("8 bytes"));
                    for (at, texel) in texel.iter_mut().enumerate() {
                        texel[3] = ((alpha >> (4 * at)) & 0xF) as u8 * 17;
                    }
                }
                Self::Bc3 => {
                    self.color_half(&block[8..], &mut texel);
                    let alphas = Self::alphas(block[0], block[1]);
                    let mut indices = [0; 8];
                    indices[..6].copy_from_slice(&block[2..8]);
                    let indices = u64::from_le_bytes(indices);
                    for (at, texel) in texel.iter_mut().enumerate() {
                        texel[3] = alphas[((indices >> (3 * at)) & 7) as usize];
                    }
                }
                Self::Rgba8 => unreachable!("RGBA8 holds no blocks"),
            }
            for row in 0..4.min(height - y) {
                for column in 0..4.min(width - x) {
                    let to = ((y + row) * width + x + column) * 4;
                    level[to..to + 4].copy_from_slice(&texel[row * 4 + column]);
                }
            }
        }
    }

    /// Writes the colors of a block's color half `half` into `texel`, each texel's of its
    /// 2-bit index, the first the top left, row after row.
    fn color_half(self, half: &[u8], texel: &mut [[u8; 4]; 16]) {
        let endpoint = |at: usize| u16::from_le_bytes([half[at], half[at + 1]]);
        let colors = self.colors(endpoint(0), endpoint(2));
        let indices = u32::from_le_bytes(half[4..8].try_into().expect("4 bytes"));
        for (at, texel) in texel.iter_mut().enumerate() {
            *texel = colors[((indices >> (2 * at)) & 3) as usize];
        }
    }

    /// The four colors of the endpoints `first` and `second`, of 5:6:5 bits: the two, then the
    /// two between at a third and two thirds; or, of a BC1 block whose first is at most its
    /// second, their mean and a transparent black.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a mean of channels of 8 bits fits 8 bits"
    )]
    fn colors(self, first: u16, second: u16) -> BlockColors {
        let widen = |color: u16| {
            let [red, green, blue] =
                [color >> 11, (color >> 5) & 0x3F, color & 0x1F].map(u32::from);
            [
                (red << 3) | (red >> 2),
                (green << 2) | (green >> 4),
                (blue << 3) | (blue >> 2),
            ]
        };
        let [a, b] = [widen(first), widen(second)];
        let mix = |weights: [u32; 2]| {
            let total = weights[0] + weights[1];
            let channel = |at: usize| (a[at] * weights[0] + b[at] * weights[1] + total / 2) / total;
            [0, 1, 2].map(channel).map(|channel| channel as u8)
        };
        let opaque = |[red, green, blue]: [u8; 3]| [red, green, blue, 255];
        if self == Self::Bc1 && first <= second {
            [
                opaque(mix([1, 0])),
                opaque(mix([0, 1])),
                opaque(mix([1, 1])),
                [0; 4],
            ]
        } else {
            [mix([1, 0]), mix([0, 1]), mix([2, 1]), mix([1, 2])].map(opaque)
        }
    }

    /// A BC3 block's eight alphas of its endpoints `first` and `second`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a mean of alphas of 8 bits fits 8 bits"
    )]
    fn alphas(first: u8, second: u8) -> [u8; 8] {
        let [a, b] = [first, second].map(u32::from);
        let mix = |steps: u32, at: u32| ((a * (steps - at) + b * at + steps / 2) / steps) as u8;
        if first > second {
            [
                first,
                second,
                mix(7, 1),
                mix(7, 2),
                mix(7, 3),
                mix(7, 4),
                mix(7, 5),
                mix(7, 6),
            ]
        } else {
            [
                first,
                second,
                mix(5, 1),
                mix(5, 2),
                mix(5, 3),
                mix(5, 4),
                0,
                255,
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_decodes_its_endpoints_the_colors_between_and_its_alpha() {
        // Endpoints of 5:6:5: red 31, green 0, blue 0 is 255, 0, 0; red 0, green 63, blue 16 is
        // 0, 255, 132, 16 widened to 0b10000_100. A third of the way: (2 · 255 + 0 + 1) / 3 = 170
        // red, (0 + 255 + 1) / 3 = 85 green, (0 + 132 + 1) / 3 = 44 blue; two thirds: 85, 170,
        // (264 + 1) / 3 = 88.
        let red = 0xF800_u16;
        let green = 0x07F0_u16;
        let four = TextureFormat::Bc3.colors(red, green);
        assert_eq!(
            four,
            [
                [255, 0, 0, 255],
                [0, 255, 132, 255],
                [170, 85, 44, 255],
                [85, 170, 88, 255]
            ]
        );
        // A BC1 block's first endpoint at most its second: the mean rounded, (255 + 1) / 2 =
        // 128, 128 and 66, then transparent; a BC1 block of the order before gives four.
        let three = TextureFormat::Bc1.colors(green, red);
        assert_eq!(three[2], [128, 128, 66, 255]);
        assert_eq!(three[3], [0; 4]);
        assert_eq!(TextureFormat::Bc1.colors(red, green), four);

        // A BC1 block of 8 × 2 texels' first level is two blocks, each cut to two rows: the
        // first's indices 0, 1, 2, 3 along its first row, the second's 3 everywhere; its first
        // endpoint is the second's, so it gives three colors and a transparent black.
        let mut bytes = Vec::new();
        bytes.extend(red.to_le_bytes());
        bytes.extend(green.to_le_bytes());
        bytes.extend(0b11_10_01_00_u32.to_le_bytes());
        bytes.extend(red.to_le_bytes());
        bytes.extend(red.to_le_bytes());
        bytes.extend(u32::MAX.to_le_bytes());
        let mut texels = vec![9];
        TextureFormat::Bc1.decode(UVec2::new(8, 2), &bytes, &mut texels);
        assert_eq!(texels.len(), 1 + 8 * 2 * 4);
        let texel = |x: usize, y: usize| &texels[1 + (y * 8 + x) * 4..][..4];
        assert_eq!(texel(0, 0), four[0]);
        assert_eq!(texel(3, 0), four[3]);
        assert_eq!(texel(0, 1), four[0]);
        assert_eq!(texel(4, 0), [0; 4]);
        assert_eq!(texel(7, 1), [0; 4]);

        // A BC3 block: alphas 255 and 0 give eight, (255 · 6 + 3) / 7 = 219 the third; of 0 and
        // 255, six and 0 and 255, (255 + 2) / 5 = 51 the third. Its color half always gives
        // four colors.
        assert_eq!(
            TextureFormat::alphas(255, 0),
            [255, 0, 219, 182, 146, 109, 73, 36]
        );
        assert_eq!(
            TextureFormat::alphas(0, 255),
            [0, 255, 51, 102, 153, 204, 0, 255]
        );
        let mut block = vec![255, 0];
        // Texel 0's alpha index 2, texel 1's 7, the rest 0.
        block.extend([0b111_010, 0, 0, 0, 0, 0]);
        block.extend(green.to_le_bytes());
        block.extend(red.to_le_bytes());
        block.extend(0b11_u32.to_le_bytes());
        let mut texels = Vec::new();
        TextureFormat::Bc3.decode(UVec2::new(4, 4), &block, &mut texels);
        let reversed = TextureFormat::Bc3.colors(green, red);
        assert_eq!(
            texels[..4],
            [reversed[3][0], reversed[3][1], reversed[3][2], 219]
        );
        assert_eq!(texels[7], 36);
        assert_eq!(texels[11], 255);
        assert_eq!(texels[8..11], reversed[0][..3]);

        // A BC2 block's alpha: 4 bits each, 0xF is 255 and 0x5 is 85.
        let mut block = 0x5F_u64.to_le_bytes().to_vec();
        block.extend([0; 8]);
        let mut texels = Vec::new();
        TextureFormat::Bc2.decode(UVec2::new(1, 1), &block, &mut texels);
        assert_eq!(texels, [0, 0, 0, 255]);
        let mut texels = Vec::new();
        TextureFormat::Bc2.decode(UVec2::new(2, 1), &block, &mut texels);
        assert_eq!(texels[7], 85);
    }
}
