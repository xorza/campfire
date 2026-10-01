use std::fmt::Write;

use super::*;
use crate::rng::rng_source::RngSource;

const WORDS_16: u64 = 1 << 16;

fn seed() -> SegmentSeed {
    SegmentSeed::new(*b"campfire rng test seed, 32 bytes")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").unwrap();
        text
    })
}

/// The keyed vectors from BLAKE3's `test_vectors.json`: key "whats the Elvish word for
/// friend", input bytes `i % 251`, 131 bytes of extended output.
#[test]
fn blake3_keyed_vectors() {
    let key = b"whats the Elvish word for friend";
    for (len, expected) in [
        (
            0,
            "92b2b75604ed3c761f9d6f62392c8a9227ad0ea3f09573e783f1498a4ed60d26b18171a2f22a4b94822c701f107153dba24918c4bae4d2945c20ece13387627d3b73cbf97b797d5e59948c7ef788f54372df45e45e4293c7dc18c1d41144a9758be58960856be1eabbe22c2653190de560ca3b2ac4aa692a9210694254c371e851bc8f",
        ),
        (
            1,
            "6d7878dfff2f485635d39013278ae14f1454b8c0a3a2d34bc1ab38228a80c95b6568c0490609413006fbd428eb3fd14e7756d90f73a4725fad147f7bf70fd61c4e0cf7074885e92b0e3f125978b4154986d4fb202a3f331a3fb6cf349a3a70e49990f98fe4289761c8602c4e6ab1138d31d3b62218078b2f3ba9a88e1d08d0dd4cea11",
        ),
        (
            64,
            "ba8ced36f327700d213f120b1a207a3b8c04330528586f414d09f2f7d9ccb7e68244c26010afc3f762615bbac552a1ca909e67c83e2fd5478cf46b9e811efccc93f77a21b17a152ebaca1695733fdb086e23cd0eb48c41c034d52523fc21236e5d8c9255306e48d52ba40b4dac24256460d56573d1312319afcf3ed39d72d0bfc69acb",
        ),
        (
            1025,
            "357dc55de0c7e382c900fd6e320acc04146be01db6a8ce7210b7189bd664ea69362396b77fdc0d2634a552970843722066c3c15902ae5097e00ff53f1e116f1cd5352720113a837ab2452cafbde4d54085d9cf5d21ca613071551b25d52e69d6c81123872b6f19cd3bc1333edf0c52b94de23ba772cf82636cff4542540a7738d5b930",
        ),
    ] {
        let input: Vec<u8> = (0..len).map(|i| u8::try_from(i % 251).unwrap()).collect();
        let mut output = [0; 131];
        Hasher::new_keyed(key)
            .update(&input)
            .finalize_xof()
            .fill(&mut output);
        assert_eq!(hex(&output), expected, "input length {len}");
    }
}

#[test]
fn words_follow_the_documented_message() {
    // "campfire/rng/v1" ‖ u32 3 ‖ "abc" ‖ u64 7 ‖ u64 9, little-endian.
    let mut message = b"campfire/rng/v1".to_vec();
    message.extend_from_slice(&[3, 0, 0, 0]);
    message.extend_from_slice(b"abc");
    message.extend_from_slice(&[7, 0, 0, 0, 0, 0, 0, 0]);
    message.extend_from_slice(&[9, 0, 0, 0, 0, 0, 0, 0]);
    // 20 words cross a block boundary, where the buffer refills.
    let mut expected = [0; 160];
    Hasher::new_keyed(seed().as_bytes())
        .update(&message)
        .finalize_xof()
        .fill(&mut expected);

    let mut rng = Rng::new(&seed(), "abc", 7, 9);
    for &chunk in expected.as_chunks::<8>().0 {
        assert_eq!(rng.next_u64(), u64::from_le_bytes(chunk));
    }
}

#[test]
fn lemire_is_uniform_over_every_16_bit_word() {
    // Exactly 2¹⁶ mod bound words are rejected, and every value takes ⌊2¹⁶ / bound⌋ words.
    for bound in [1, 2, 3, 5, 7, 10, 100, 1000, 4097, 65_535, 65_536] {
        let mut counts = vec![0_u64; usize::try_from(bound).unwrap()];
        let mut rejected = 0;
        for word in 0..WORDS_16 {
            match lemire_step::<16>(word, bound) {
                Some(value) => counts[usize::try_from(value).unwrap()] += 1,
                None => rejected += 1,
            }
        }
        assert_eq!(rejected, WORDS_16 % bound, "bound {bound}");
        assert!(
            counts.iter().all(|&count| count == WORDS_16 / bound),
            "bound {bound}"
        );
    }
}

#[test]
fn chance_ratio_is_exact_over_every_16_bit_word() {
    // Among accepted words, those whose value is below the numerator make exactly
    // numerator / denominator of them.
    for (numerator, denominator) in [
        (0, 1),
        (1, 1),
        (1, 3),
        (2, 3),
        (1, 1000),
        (999, 1000),
        (5, 4),
    ] {
        let mut hits = 0;
        let mut accepted = 0;
        for word in 0..WORDS_16 {
            if let Some(value) = lemire_step::<16>(word, denominator) {
                accepted += 1;
                hits += u64::from(value < numerator);
            }
        }
        assert_eq!(
            hits * denominator,
            numerator.min(denominator) * accepted,
            "{numerator}/{denominator}"
        );
    }
}

#[test]
fn chance_compares_the_top_24_bits() {
    let top = |bits: u64| bits << 40;
    let epsilon = Num::EPSILON;
    let almost_one = Num::ONE - Num::EPSILON;
    assert!(chance_from_word(top(0), epsilon));
    assert!(!chance_from_word(top(1), epsilon));
    assert!(chance_from_word(top((1 << 24) - 2), almost_one));
    assert!(!chance_from_word(top((1 << 24) - 1), almost_one));
    assert!(!chance_from_word(0, Num::ZERO));
    assert!(!chance_from_word(0, -Num::ONE));
    assert!(chance_from_word(u64::MAX, Num::ONE));
    assert!(chance_from_word(u64::MAX, Num::MAX));

    // A fraction of the same word is below a probability exactly when the chance is true: at the
    // ends, at a tie, and either side of it.
    assert_eq!(fraction_of_word(top(3) | 0xFF), Num::from_bits(3));
    assert_eq!(fraction_of_word(u64::MAX), almost_one);
    let words = [
        0,
        top(1) - 1,
        top(1),
        top(1 << 23),
        top((1 << 24) - 1),
        u64::MAX,
    ];
    let chances = [
        -Num::ONE,
        Num::ZERO,
        epsilon,
        Num::ONE / 2,
        almost_one,
        Num::ONE,
        Num::MAX,
    ];
    for word in words {
        for probability in chances {
            let below = fraction_of_word(word) < probability;
            assert_eq!(
                below,
                chance_from_word(word, probability),
                "{word:#x} {probability:?}"
            );
        }
    }
}

#[test]
fn chance_always_takes_one_word() {
    let mut a = Rng::new(&seed(), "s", 1, 1);
    let mut b = Rng::new(&seed(), "s", 1, 1);
    assert!(!a.chance(Num::ZERO));
    b.next_u64();
    assert_eq!(a.next_u64(), b.next_u64());
}

#[test]
fn pick_and_below_stay_in_range() {
    let mut rng = Rng::new(&seed(), "s", 1, 1);
    assert_eq!(rng.pick(1), 0);
    assert_eq!(rng.below(1), 0);
    for _ in 0..1000 {
        assert!(rng.pick(3) < 3);
        assert!(rng.below(u64::MAX) < u64::MAX);
    }
}

#[test]
fn sequences_depend_on_every_field() {
    let first =
        |stream: &str, entity: u64, tick: u64| Rng::new(&seed(), stream, entity, tick).next_u64();
    let base = first("s", 1, 1);
    assert_eq!(base, first("s", 1, 1));
    assert_ne!(base, first("t", 1, 1));
    assert_ne!(base, first("s", 2, 1));
    assert_ne!(base, first("s", 1, 2));
    assert_ne!(
        base,
        Rng::new(&SegmentSeed::new([1; 32]), "s", 1, 1).next_u64()
    );
}

#[test]
fn source_opens_each_pair_once_per_tick() {
    let mut source = RngSource::new(seed());
    source.begin_tick(5);
    let a = source.open("s", 1).next_u64();
    assert_ne!(source.open("s", 2).next_u64(), a);
    assert_ne!(source.open("t", 1).next_u64(), a);
    source.begin_tick(5);
    assert_eq!(source.open("s", 1).next_u64(), a);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "opened twice for entity 1 in tick 5")]
fn source_panics_on_a_repeated_pair() {
    let mut source = RngSource::new(seed());
    source.begin_tick(5);
    let _first = source.open("s", 1);
    let _second = source.open("s", 1);
}
