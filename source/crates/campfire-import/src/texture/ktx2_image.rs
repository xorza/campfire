use crate::texture::texture_format::TextureFormat;

/// A 2D texture as a KTX2 file of no supercompression (the Khronos KTX 2.0 specification): its
/// format, its base size, and its levels from the base down, each `format.level_bytes` of its
/// size.
#[derive(Debug)]
pub(crate) struct Ktx2Image<'a> {
    pub(crate) format: TextureFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) levels: &'a [&'a [u8]],
}

/// KTX2's identifier: `«KTX 20»\r\n\x1A\n`.
const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];

/// The bytes before the level index: the identifier, nine header words, and the index of four
/// words and two 64-bit words.
const LEVEL_INDEX: usize = 80;

/// A level index entry's bytes: its offset, its length and its length unpacked, each 64-bit.
const LEVEL_ENTRY: usize = 24;

/// A basic descriptor block's bytes before its samples, and a sample's.
const BLOCK_HEAD: u32 = 24;
const SAMPLE: u32 = 16;

/// The descriptor's version, 1.3's, and its BT.709 primaries and sRGB transfer function.
const VERSION: u32 = 2;
const BT709: u32 = 1;
const SRGB: u32 = 2;

impl Ktx2Image<'_> {
    /// The file's bytes: its header, its index, its level index, its data format descriptor and
    /// no key/value data, then its levels, the smallest first, each aligned to the lcm of its
    /// format's block bytes and 4.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let count = self.levels.len();
        debug_assert!(count > 0, "a texture has a base level");
        let (mut width, mut height) = (self.width, self.height);
        for level in self.levels {
            debug_assert_eq!(Some(level.len()), self.format.level_bytes(width, height));
            (width, height) = ((width / 2).max(1), (height / 2).max(1));
        }
        let samples = self.format.samples();
        let block = BLOCK_HEAD + SAMPLE * u32::try_from(samples.len()).expect("few samples");
        let dfd_at = LEVEL_INDEX + LEVEL_ENTRY * count;
        let dfd_len = 4 + block;
        let word = |out: &mut Vec<u8>, value: u32| out.extend(value.to_le_bytes());
        let wide = |out: &mut Vec<u8>, value: usize| {
            out.extend(
                u64::try_from(value)
                    .expect("a length fits u64")
                    .to_le_bytes(),
            );
        };

        let mut out = IDENTIFIER.to_vec();
        for value in [
            self.format.vk_format(),
            1,
            self.width,
            self.height,
            0,
            0,
            1,
            u32::try_from(count).expect("few levels"),
            0,
        ] {
            word(&mut out, value);
        }
        word(&mut out, u32::try_from(dfd_at).expect("a small index"));
        word(&mut out, dfd_len);
        word(&mut out, 0);
        word(&mut out, 0);
        wide(&mut out, 0);
        wide(&mut out, 0);

        // The levels' places: after the descriptor, the smallest first, each aligned.
        // Each format's block bytes are a multiple of 4, so they are the lcm.
        let align = usize::try_from(self.format.block_bytes()).expect("a small block");
        let mut at = dfd_at + dfd_len as usize;
        let mut places = vec![0; count];
        for (level, bytes) in self.levels.iter().enumerate().rev() {
            at = at.next_multiple_of(align);
            places[level] = at;
            at += bytes.len();
        }
        for (place, bytes) in places.iter().zip(self.levels) {
            wide(&mut out, *place);
            wide(&mut out, bytes.len());
            wide(&mut out, bytes.len());
        }

        // A basic descriptor block, version 2, Khronos' vendor and type 0: the model, BT.709
        // primaries, the sRGB transfer function and straight alpha; the block's sides less 1;
        // its bytes in plane 0; then each sample.
        word(&mut out, dfd_len);
        word(&mut out, 0);
        word(&mut out, VERSION | (block << 16));
        word(
            &mut out,
            u32::from(self.format.model()) | (BT709 << 8) | (SRGB << 16),
        );
        let side = self.format.block_side() - 1;
        word(&mut out, side | (side << 8));
        word(&mut out, self.format.block_bytes());
        word(&mut out, 0);
        for sample in samples {
            word(
                &mut out,
                u32::from(sample.offset)
                    | (u32::from(sample.bits - 1) << 16)
                    | (u32::from(sample.channel) << 24),
            );
            word(&mut out, 0);
            word(&mut out, 0);
            word(&mut out, sample.upper);
        }

        for (place, bytes) in places.iter().zip(self.levels).rev() {
            out.resize(*place, 0);
            out.extend_from_slice(bytes);
        }
        out
    }
}
