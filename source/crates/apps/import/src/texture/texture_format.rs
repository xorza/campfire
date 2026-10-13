/// A texture's format in a package, every one sRGB-encoded color: the block formats a DDS's DXT
/// blocks are, or 8-bit RGBA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextureFormat {
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

impl TextureFormat {
    /// Vulkan's number for it, `VK_FORMAT_BC1_RGBA_SRGB_BLOCK` and the rest.
    pub(crate) const fn vk_format(self) -> u32 {
        match self {
            TextureFormat::Bc1 => 134,
            TextureFormat::Bc2 => 136,
            TextureFormat::Bc3 => 138,
            TextureFormat::Rgba8 => 43,
        }
    }

    /// The side of a block, in texels.
    pub(crate) const fn block_side(self) -> u32 {
        match self {
            TextureFormat::Bc1 | TextureFormat::Bc2 | TextureFormat::Bc3 => 4,
            TextureFormat::Rgba8 => 1,
        }
    }

    /// The bytes of a block.
    pub(crate) const fn block_bytes(self) -> u32 {
        match self {
            TextureFormat::Bc1 => 8,
            TextureFormat::Bc2 | TextureFormat::Bc3 => 16,
            TextureFormat::Rgba8 => 4,
        }
    }

    /// The bytes of a level of `width` by `height` texels: its blocks, a part block counted
    /// whole; `None` past what `usize` holds.
    pub(crate) const fn level_bytes(self, width: u32, height: u32) -> Option<usize> {
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
            TextureFormat::Bc1 => 128,
            TextureFormat::Bc2 => 129,
            TextureFormat::Bc3 => 130,
            TextureFormat::Rgba8 => 1,
        }
    }

    /// Its samples, as Khronos' descriptor of the Vulkan format lists them: a BC1 block's one,
    /// with alpha present; a BC2 or BC3 block's alpha then color; 8-bit red, green, blue and
    /// alpha. Alpha is linear.
    pub(crate) const fn samples(self) -> &'static [Sample] {
        const BLOCK: u32 = u32::MAX;
        match self {
            TextureFormat::Bc1 => &[Sample {
                offset: 0,
                bits: 64,
                channel: 1,
                upper: BLOCK,
            }],
            TextureFormat::Bc2 | TextureFormat::Bc3 => &[
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
            TextureFormat::Rgba8 => &[
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
