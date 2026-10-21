use super::*;
use crate::cell_key::CellKey;

#[test]
fn a_reach_takes_the_least_level_twice_as_wide() {
    // A base of 1 m: twice 0 and 0.5 m fit it, level 0; twice 0.5 m and a bit takes 2 m,
    // level 1, as does twice 1 m; twice 1.2 m, 2.4 m, takes 4 m, level 2; twice 2,048 m,
    // 4,096 m = 2¹² m, level 12.
    let levels = CellLevels::new(Num::ONE);
    let bit = Num::from_bits(1);
    for (reach, level) in [
        (Num::ZERO, 0),
        (Num::HALF, 0),
        (Num::HALF + bit, 1),
        (Num::ONE, 1),
        (Num::int(12) / 10, 2),
        (Num::int(2048), 12),
    ] {
        assert_eq!(levels.level(reach), level, "{reach:?}");
        assert!(levels.cell(level) >= 2 * reach.to_bits());
        assert!(level == 0 || levels.cell(level - 1) < 2 * reach.to_bits());
    }
    assert_eq!(levels.cell(12), Num::int(4096).to_bits());
    // A base of 0.7 m, for walkers of 0.35 m: 2,048 m takes 0.7 · 2¹³ = 5,734.4 m, where
    // 2¹² · 0.7 = 2,867.2 m falls short of 4,096.
    let walkers = CellLevels::new(Num::int(7) / 10);
    assert_eq!(walkers.level(Num::int(2048)), 13);
    // A base narrower than an eighth of a meter takes an eighth.
    assert_eq!(
        CellLevels::new(Num::from_bits(5)),
        CellLevels::new(CellLevels::MIN_BASE)
    );
    assert_eq!(
        CellLevels::new(Num::from_bits(5)).cell(0),
        Num::ONE.to_bits() / 8
    );
    // Rows round down on both sides of 0, and nest: a cell's row a level up is its own
    // halved, rounded down, and a cell key's `within` finds the cell `index` gives there.
    assert_eq!(
        [-3, -1, 0, 1, 3].map(|m| levels.index(1, Num::int(m).to_bits())),
        [-2, -1, 0, 0, 1]
    );
    for bits in [-5 << 24, -1, 0, 3 << 23, 17 << 24] {
        assert_eq!(levels.index(1, bits), levels.index(0, bits) >> 1, "{bits}");
        let key = CellKey::new(0, 0, levels.index(0, bits), levels.index(0, -bits));
        for level in 1..4 {
            let up = key.within(level);
            assert_eq!(
                (up.row(), up.column()),
                (levels.index(level, bits), levels.index(level, -bits)),
                "{bits} at {level}"
            );
        }
    }
}
