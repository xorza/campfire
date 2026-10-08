use super::*;

/// Sets `cells` in the bitmap at `run` of `words`, none hidden, marking the words in `set`.
fn set_bits(words: &mut [u64], set: &mut [u64], run: usize, cells: Range<usize>) {
    Bitmap { words, set, run }.set(cells, None);
}

#[test]
fn set_bits_fills_a_run_within_one_word_and_across_words_and_notes_them() {
    let (mut words, mut set) = ([0; 3], [0]);
    set_bits(&mut words, &mut set, 0, 3..5);
    assert_eq!(words, [0b11000, 0, 0]);
    set_bits(&mut words, &mut set, 0, 63..64);
    assert_eq!(words, [0b11000 | 1 << 63, 0, 0]);
    assert_eq!(set, [0b1]);
    // Bits 60 to 130: the top 4 of word 0, all of word 1, and the low 3 of word 2; at run
    // 1, word 0 of the map is word 1.
    let (mut words, mut set) = ([0; 4], [0]);
    set_bits(&mut words, &mut set, 1, 60..131);
    assert_eq!(words, [0, 0xF << 60, u64::MAX, 0b111]);
    assert_eq!(set, [0b1110]);
}

#[test]
fn set_bits_sets_exactly_its_run_for_every_run_of_three_words() {
    // Every run `s..e` with 0 ≤ s < e ≤ 192, at run 0 and 1, against a bit-by-bit fill,
    // over bits already set elsewhere, which stay; the words it touches are marked.
    for run in 0..2 {
        for start in 0..192 {
            for end in start + 1..=192 {
                let mut words = [0x8000_0000_0000_0001_u64; 4];
                let mut expected = words;
                for bit in start..end {
                    expected[run + bit / 64] |= 1 << (bit % 64);
                }
                let mut set = [0];
                set_bits(&mut words, &mut set, run, start..end);
                assert_eq!(words, expected, "{run} {start}..{end}");
                let touched = (run + start / 64..=run + (end - 1) / 64).map(|word| 1 << word);
                assert_eq!(set, [touched.sum::<u64>()], "{run} {start}..{end}");
            }
        }
    }
}

#[test]
fn a_tick_clears_only_what_the_tick_before_revealed() {
    // Two groups over 200 cells, 4 words each. Group 1 sees cells 60 to 69, across words 0
    // and 1, and detects 130; group 0 detects nothing.
    let mut maps = SightMaps::default();
    maps.reset(200, 2);
    maps.reveal(1, 60..70, false, None);
    maps.reveal(1, 130..131, true, None);
    let seen =
        |maps: &SightMaps, group, cell: usize, hidden| maps.sees(group, cell..cell + 1, hidden);
    assert!(seen(&maps, 1, 60, false) && seen(&maps, 1, 69, false));
    assert!(!seen(&maps, 1, 59, false) && !seen(&maps, 1, 70, false));
    assert!(!seen(&maps, 0, 65, false));
    assert!(seen(&maps, 1, 130, true) && !seen(&maps, 1, 65, true));
    assert!(!seen(&maps, 0, 130, true));
    // The next tick starts clear: only the words set were cleared, and they were all set.
    maps.begin_tick();
    assert!(
        maps.revealed
            .iter()
            .chain(&maps.detected)
            .all(|&word| word == 0)
    );
    let marked = maps.set_revealed.iter().chain(&maps.set_detected);
    assert!(marked.copied().all(|marks| marks == 0));
    // Two maps of 4 words take one word of marks; group 1's detection takes the next slot's.
    assert_eq!((maps.set_revealed.len(), maps.set_detected.len()), (1, 1));
    // Group 0 then sees a run across three words: 0 to 191.
    maps.reveal(0, 0..192, false, None);
    assert_eq!(maps.set_revealed, [0b111]);
    assert!(seen(&maps, 0, 191, false) && !seen(&maps, 0, 192, false));
}
