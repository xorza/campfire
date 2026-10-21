use campfire_math::Num;

/// Square cells of the ground plane in levels by size, as a hierarchical grid keeps them
/// (Mirtich's thesis; Ericson, chapter 7.2): each level's cell is `2^RATIO_BITS` times the one
/// below, from the base cell, so a wide body costs its own few cells and not every body's. A body
/// goes into the least level whose cell is at least twice its reach from its center along each
/// axis, so it covers at most two cells along each. Level `k`'s cells nest in level `k + 1`'s: a
/// cell's row and column there are its own shifted right by `RATIO_BITS`, as the floor of a
/// division by the ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellLevels {
    /// The base cell, in raw units.
    base: i64,
}

impl CellLevels {
    /// The narrowest base cell, an eighth of a meter, 2²¹ bits: the rows and columns of positions
    /// within `Vec3::BOUND`, 2⁴⁴ bits, then lie within ±2²³, as a `CellKey` holds them.
    pub const MIN_BASE: Num = Num::from_bits(1 << 21);

    /// The power of two by which each level's cell is wider than the one below
    /// (04-capabilities/00-overview.md, Spatial indexes in levels by size).
    pub const RATIO_BITS: u32 = 1;

    /// Levels from a base cell of `base`, or of `MIN_BASE` when `base` is narrower.
    pub const fn new(base: Num) -> Self {
        let min = Self::MIN_BASE.to_bits();
        Self {
            base: if base.to_bits() < min {
                min
            } else {
                base.to_bits()
            },
        }
    }

    /// The least level whose cell is at least twice `reach`.
    pub const fn level(self, reach: Num) -> u8 {
        debug_assert!(reach.to_bits() >= 0);
        let need = reach.to_bits() * 2;
        let mut level = 0;
        while self.cell(level) < need {
            level += 1;
        }
        level
    }

    /// The side of `level`'s cells, in raw units.
    pub const fn cell(self, level: u8) -> i64 {
        self.base << (Self::RATIO_BITS * level as u32)
    }

    /// The row or column of `level` that holds `bits` along its axis.
    pub const fn index(self, level: u8, bits: i64) -> i64 {
        bits.div_euclid(self.cell(level))
    }
}

#[cfg(test)]
mod tests;
