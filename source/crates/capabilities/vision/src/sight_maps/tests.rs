use super::*;

impl SightMaps {
    /// Whether every bit and every count is 0.
    pub(crate) fn is_clear(&self) -> bool {
        self.revealed.is_clear() && self.detected.is_clear()
    }
}

/// A looker of `group` that hides nothing.
const fn looker(group: usize, detects: bool) -> Looker<'static> {
    Looker {
        group,
        detects,
        hidden: None,
    }
}

#[test]
fn a_cell_is_seen_while_its_count_is_above_0() {
    // Two groups over 200 cells, 4 words each. Group 1 sees cells 60 to 69, across words 0 and 1,
    // twice, and detects 130; group 0 sees and detects nothing.
    let mut maps = SightMaps::default();
    maps.reset(200, 2);
    maps.look(looker(1, false), 60..70, RunChange::Gained);
    maps.look(looker(1, false), 65..70, RunChange::Gained);
    maps.look(looker(1, true), 130..131, RunChange::Gained);
    let seen =
        |maps: &SightMaps, group, cell: usize, hidden| maps.sees(group, cell..cell + 1, hidden);
    assert!(seen(&maps, 1, 60, false) && seen(&maps, 1, 69, false));
    assert!(!seen(&maps, 1, 59, false) && !seen(&maps, 1, 70, false));
    assert!(!seen(&maps, 0, 65, false));
    assert!(seen(&maps, 1, 130, true) && !seen(&maps, 1, 65, true));
    assert!(!seen(&maps, 0, 130, true));
    // The first sight leaves: 60 to 64 go dark, 65 to 69 stay, as the second counts on them.
    maps.look(looker(1, false), 60..70, RunChange::Lost);
    assert!(!maps.sees(1, 60..65, false));
    assert!(seen(&maps, 1, 65, false) && seen(&maps, 1, 69, false));
    // The others leave: every bit and count is 0 again.
    maps.look(looker(1, false), 65..70, RunChange::Lost);
    maps.look(looker(1, true), 130..131, RunChange::Lost);
    assert!(maps.is_clear());
}

#[test]
fn a_count_sets_exactly_its_run_for_every_run_of_three_words() {
    // Every run `s..e` with 0 ≤ s < e ≤ 192 counted on map 1 of 2, over cells 0 and 191 already
    // seen, against a bit-by-bit fill; taken back, it leaves only those two.
    let mut maps = SightMaps::default();
    maps.reset(192, 2);
    for cell in [0, 191] {
        maps.look(looker(1, false), cell..cell + 1, RunChange::Gained);
    }
    let before = [1, 0, 1 << 63];
    for start in 0..192 {
        for end in start + 1..=192 {
            maps.look(looker(1, false), start..end, RunChange::Gained);
            let mut expected = before;
            for bit in start..end {
                expected[bit / 64] |= 1 << (bit % 64);
            }
            assert_eq!(maps.revealed.bits(1), expected, "{start}..{end}");
            assert_eq!(maps.revealed.bits(0), [0; 3], "{start}..{end}");
            maps.look(looker(1, false), start..end, RunChange::Lost);
            assert_eq!(maps.revealed.bits(1), before, "{start}..{end}");
        }
    }
}
