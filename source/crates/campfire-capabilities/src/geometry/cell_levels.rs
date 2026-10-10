use campfire_math::Num;

/// Square cells of the ground plane in levels by size, as a hierarchical grid keeps them
/// (Mirtich's thesis; Ericson, chapter 7.2): each level's cell is twice the one below, from the
/// base cell, so a wide body costs its own few cells and not every body's. A body goes into the
/// least level whose cell is at least twice its reach from its center along each axis, so it
/// covers at most two cells along each. Level `k`'s cells nest in level `k + 1`'s: a cell's row
/// and column there are its own shifted right by one, as the floor of a halving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellLevels {
    /// The base cell, in raw units.
    base: i64,
}

impl CellLevels {
    /// Levels from a base cell of `base`, positive.
    pub(crate) const fn new(base: Num) -> CellLevels {
        debug_assert!(base.to_bits() > 0, "a cell of a positive side");
        CellLevels {
            base: base.to_bits(),
        }
    }

    /// The least level whose cell is at least twice `reach`.
    pub(crate) const fn level(self, reach: Num) -> u8 {
        debug_assert!(reach.to_bits() >= 0);
        let need = reach.to_bits() * 2;
        let mut level = 0;
        while self.cell(level) < need {
            level += 1;
        }
        level
    }

    /// The side of `level`'s cells, in raw units.
    pub(crate) const fn cell(self, level: u8) -> i64 {
        self.base << level
    }

    /// The row or column of `level` that holds `bits` along its axis.
    pub(crate) const fn index(self, level: u8, bits: i64) -> i64 {
        bits.div_euclid(self.cell(level))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Rows round down on both sides of 0, and nest: a cell's row a level up is its own
        // halved, rounded down.
        assert_eq!(
            [-3, -1, 0, 1, 3].map(|m| levels.index(1, Num::int(m).to_bits())),
            [-2, -1, 0, 0, 1]
        );
        for bits in [-5 << 24, -1, 0, 3 << 23, 17 << 24] {
            assert_eq!(levels.index(1, bits), levels.index(0, bits) >> 1, "{bits}");
        }
    }
}
