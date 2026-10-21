use super::*;
use crate::texture::dds_file::samples::dds;

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
        read(&[dds(*b"DXT1", 8, 8, 4, &blocks).as_slice(), &[0]].concat()),
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
