/// A cell of a layer's level of `CellLevels`, packed into one number so that cells order as their
/// layer, level, row and column do, and a sort or a search compares one number: the layer in the
/// top 8 bits, the level in the next 6, and the row and the column, each offset by 2²⁴, in 25 bits
/// each. A base cell of at least `CellLevels::MIN_BASE` holds the rows and columns of positions
/// within `Position::BOUND` to ±2²³, and so of the cells beside them; a search's rows and columns
/// past ±2²⁴ clamp to the ends, where no body is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellKey(u64);

impl CellKey {
    /// The offset of a row or a column.
    const MIDDLE: i64 = 1 << 24;
    const PLACE_BITS: u32 = 25;
    const PLACE_MASK: u64 = (1 << CellKey::PLACE_BITS) - 1;
    const LEVEL_SHIFT: u32 = 2 * CellKey::PLACE_BITS;
    const LAYER_SHIFT: u32 = CellKey::LEVEL_SHIFT + 6;

    /// The cell of `row` and `column` of `level` of the layer `layer`, each within its bits.
    pub const fn new(layer: u8, level: u8, row: i64, column: i64) -> CellKey {
        debug_assert!(level < 1 << 6, "a level fits 6 bits");
        debug_assert!(-CellKey::MIDDLE <= row && row < CellKey::MIDDLE);
        debug_assert!(-CellKey::MIDDLE <= column && column < CellKey::MIDDLE);
        CellKey::pack(layer, level, row, column)
    }

    /// The cell of `row` and `column` of `level` of the layer `layer`, a row or a column past its
    /// bits clamped to its end, as a search's bounds may be.
    pub const fn clamped(layer: u8, level: u8, row: i64, column: i64) -> CellKey {
        debug_assert!(level < 1 << 6, "a level fits 6 bits");
        CellKey::pack(layer, level, CellKey::clamp(row), CellKey::clamp(column))
    }

    pub const fn from_bits(bits: u64) -> CellKey {
        CellKey(bits)
    }

    pub const fn to_bits(self) -> u64 {
        self.0
    }

    pub const fn layer(self) -> u8 {
        (self.0 >> CellKey::LAYER_SHIFT) as u8
    }

    pub const fn level(self) -> u8 {
        ((self.0 >> CellKey::LEVEL_SHIFT) & 0x3f) as u8
    }

    pub const fn row(self) -> i64 {
        CellKey::unplace(self.0 >> CellKey::PLACE_BITS)
    }

    pub const fn column(self) -> i64 {
        CellKey::unplace(self.0)
    }

    /// Whether `other` is of this one's layer and level.
    pub const fn same_level(self, other: CellKey) -> bool {
        self.0 >> CellKey::LEVEL_SHIFT == other.0 >> CellKey::LEVEL_SHIFT
    }

    /// The cell `rows` below and `columns` to the right of this one.
    #[must_use]
    pub const fn beside(self, rows: i64, columns: i64) -> CellKey {
        CellKey::new(
            self.layer(),
            self.level(),
            self.row() + rows,
            self.column() + columns,
        )
    }

    /// The cell of `level`, at or above this one's, that holds this one, as a halving's floor
    /// nests a level's cells in the next one's.
    #[must_use]
    pub const fn within(self, level: u8) -> CellKey {
        debug_assert!(level >= self.level());
        let up = level - self.level();
        CellKey::new(self.layer(), level, self.row() >> up, self.column() >> up)
    }

    const fn pack(layer: u8, level: u8, row: i64, column: i64) -> CellKey {
        CellKey(
            ((layer as u64) << CellKey::LAYER_SHIFT)
                | ((level as u64) << CellKey::LEVEL_SHIFT)
                | (CellKey::place(row) << CellKey::PLACE_BITS)
                | CellKey::place(column),
        )
    }

    const fn clamp(index: i64) -> i64 {
        if index < -CellKey::MIDDLE {
            -CellKey::MIDDLE
        } else if index >= CellKey::MIDDLE {
            CellKey::MIDDLE - 1
        } else {
            index
        }
    }

    /// A row or a column, offset into its bits.
    const fn place(index: i64) -> u64 {
        (index + CellKey::MIDDLE).cast_unsigned()
    }

    /// The row or column in the low bits of `bits`.
    const fn unplace(bits: u64) -> i64 {
        (bits & CellKey::PLACE_MASK).cast_signed() - CellKey::MIDDLE
    }
}

#[cfg(test)]
mod tests {
    use crate::cell_key::CellKey;

    #[test]
    fn a_key_orders_as_its_layer_level_row_and_column_and_reads_them_back() {
        let bound = CellKey::MIDDLE;
        let places = [-bound, -bound + 1, -3, -1, 0, 1, 7, bound - 2, bound - 1];
        let mut keys = Vec::new();
        for layer in [0, 1, 255] {
            for level in [0, 1, 63] {
                for row in places {
                    for column in places {
                        let key = CellKey::new(layer, level, row, column);
                        assert_eq!(
                            (key.layer(), key.level(), key.row(), key.column()),
                            (layer, level, row, column)
                        );
                        assert_eq!(CellKey::from_bits(key.to_bits()), key);
                        keys.push(key);
                    }
                }
            }
        }
        // Built in the order of their parts, the keys come out sorted.
        assert!(keys.is_sorted());
        let key = CellKey::new(3, 2, -5, 9);
        assert_eq!(key.beside(1, -1), CellKey::new(3, 2, -4, 8));
        assert!(key.same_level(CellKey::new(3, 2, 100, -100)));
        assert!(!key.same_level(CellKey::new(3, 3, -5, 9)));
        assert!(!key.same_level(CellKey::new(4, 2, -5, 9)));
        // Halved twice, rounded down: -5 → -2, 9 → 2.
        assert_eq!(key.within(4), CellKey::new(3, 4, -2, 2));
        assert_eq!(key.within(2), key);
        // Past the bits, a row or a column clamps to its end.
        assert_eq!(
            CellKey::clamped(0, 1, i64::MIN, i64::MAX),
            CellKey::new(0, 1, -bound, bound - 1)
        );
        assert_eq!(CellKey::clamped(0, 1, -5, 9), CellKey::new(0, 1, -5, 9));
    }
}
