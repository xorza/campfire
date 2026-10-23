use campfire_math::Num;

use crate::zero_hour::axes;

/// Where a map's samples lie on the engine's axes: `cell` apart, the game's frame at the sample
/// `[border, border]`, each turned as `axes::ground` turns a point, so the engine's `z` runs back
/// along the game's rows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CellFrame {
    pub(crate) border: usize,
    pub(crate) cell: Num,
}

impl CellFrame {
    /// The engine's point of the game's sample `[column, row]`; none past the world's bound.
    pub(crate) fn sample(self, [column, row]: [usize; 2]) -> Option<[Num; 2]> {
        let along = |sample: usize| {
            let cells = i64::try_from(sample).ok()? - i64::try_from(self.border).ok()?;
            self.cell.checked_mul_int(cells)
        };
        axes::ground([along(column)?, along(row)?])
    }
}
