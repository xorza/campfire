use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A map's ground, `map/<name>/heights.bin`, through `Binary`: samples `cell` meters apart, the one of
/// `column` and `row` at `[x, z] = origin + [column, row] · cell`, row after row from the first,
/// each `step` meters per unit above height 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeightGrid {
    origin: [Num; 2],
    cell: Num,
    step: Num,
    columns: u32,
    samples: Vec<u8>,
}

impl HeightGrid {
    /// The grid; `None` unless `cell` and `step` are positive, `samples` fill at least two whole
    /// rows of at least two columns, and every sample, at its highest, lies within the world's
    /// bound.
    pub fn new(
        origin: [Num; 2],
        cell: Num,
        step: Num,
        columns: u32,
        samples: Vec<u8>,
    ) -> Option<HeightGrid> {
        let width = usize::try_from(columns).ok()?;
        if cell <= Num::ZERO
            || step <= Num::ZERO
            || width < 2
            || !samples.len().is_multiple_of(width)
        {
            return None;
        }
        let rows = u32::try_from(samples.len() / width).ok()?;
        if rows < 2 {
            return None;
        }
        let grid = HeightGrid {
            origin,
            cell,
            step,
            columns,
            samples,
        };
        let far = grid.checked_far_corner()?;
        let top = step.checked_mul_int(u8::MAX.into())?;
        let within = |[x, z]: [Num; 2]| Position::new(Vec3::new(x, top, z)).is_some();
        (within(origin) && within(far)).then_some(grid)
    }

    /// The ground position of the first sample.
    pub const fn origin(&self) -> [Num; 2] {
        self.origin
    }

    /// The ground position of the last sample.
    pub fn far_corner(&self) -> [Num; 2] {
        self.checked_far_corner()
            .expect("a grid within the world's bound")
    }

    pub const fn cell(&self) -> Num {
        self.cell
    }

    pub const fn step(&self) -> Num {
        self.step
    }

    pub const fn columns(&self) -> u32 {
        self.columns
    }

    pub fn rows(&self) -> u32 {
        u32::try_from(self.samples.len() / self.columns as usize).expect("rows fit u32")
    }

    /// Each sample, row after row.
    pub fn samples(&self) -> &[u8] {
        &self.samples
    }

    fn checked_far_corner(&self) -> Option<[Num; 2]> {
        let at = |origin: Num, count: u32| {
            self.cell
                .checked_mul_int(i64::from(count) - 1)
                .and_then(|span| origin.checked_add(span))
        };
        Some([
            at(self.origin[0], self.columns)?,
            at(self.origin[1], self.rows())?,
        ])
    }
}

/// A package's file is untrusted, so a grid `new` refuses fails to decode.
impl<'de> Deserialize<'de> for HeightGrid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<HeightGrid, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            origin: [Num; 2],
            cell: Num,
            step: Num,
            columns: u32,
            samples: Vec<u8>,
        }
        let fields = Fields::deserialize(deserializer)?;
        HeightGrid::new(
            fields.origin,
            fields.cell,
            fields.step,
            fields.columns,
            fields.samples,
        )
        .ok_or_else(|| {
            D::Error::custom(
                "a height grid needs a positive cell and step, at least 2 × 2 whole samples, \
                 and every sample within the world's bound",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_common::{Binary, BinaryError};

    #[test]
    fn a_grid_encodes_its_fields_and_decodes_only_as_new_takes_it() {
        let half = Num::HALF;
        let grid = HeightGrid::new(
            [Num::ZERO, Num::int(-3)],
            Num::ONE,
            half,
            2,
            vec![0, 1, 2, 3],
        )
        .unwrap();
        assert_eq!(grid.rows(), 2);
        assert_eq!(grid.far_corner(), [Num::ONE, Num::int(-2)]);
        // Postcard writes each `Num` as its bits in zigzag varints: 0 as 0; −3 · 2²⁴ as
        // 2 · 3 · 2²⁴ − 1 = 0x5FF_FFFF, seven bits a byte from the lowest; 1 as 2²⁵; ½ as 2²⁴.
        // Then the columns, and the samples' count and bytes.
        let bytes = [
            &[0x00][..],
            &[0xFF, 0xFF, 0xFF, 0x2F],
            &[0x80, 0x80, 0x80, 0x10],
            &[0x80, 0x80, 0x80, 0x08],
            &[2, 4, 0, 1, 2, 3],
        ]
        .concat();
        assert_eq!(Binary::encode(&grid), bytes);
        assert_eq!(Binary::decode::<HeightGrid>(&bytes).unwrap(), grid);

        // A cell or a step of 0, a ragged or a single row, one column, and a grid past the
        // world's bound, 2²⁰ m, at its far corner or at its highest.
        let bound = Position::BOUND;
        let refused = [
            HeightGrid::new([Num::ZERO; 2], Num::ZERO, half, 2, vec![0; 4]),
            HeightGrid::new([Num::ZERO; 2], Num::ONE, Num::ZERO, 2, vec![0; 4]),
            HeightGrid::new([Num::ZERO; 2], Num::ONE, half, 2, vec![0; 5]),
            HeightGrid::new([Num::ZERO; 2], Num::ONE, half, 2, vec![0; 2]),
            HeightGrid::new([Num::ZERO; 2], Num::ONE, half, 1, vec![0; 4]),
            HeightGrid::new([bound, Num::ZERO], Num::ONE, half, 2, vec![0; 4]),
            HeightGrid::new([Num::ZERO; 2], Num::ONE, bound, 2, vec![0; 4]),
        ];
        assert!(refused.iter().all(Option::is_none), "{refused:?}");
        let edge = HeightGrid::new([bound - Num::ONE, Num::ZERO], Num::ONE, half, 2, vec![0; 4]);
        assert_eq!(edge.unwrap().far_corner(), [bound, Num::ONE]);
        let ragged = [&bytes[..13], &[2, 3, 0, 1, 2]].concat();
        assert!(matches!(
            Binary::decode::<HeightGrid>(&ragged),
            Err(BinaryError::Malformed(_))
        ));
        // The same grid with its column count over-long, 2 as 0x82 0x00, or a byte past its end.
        let long = [&bytes[..13], &[0x82, 0x00], &bytes[14..]].concat();
        let trailing = [&bytes[..], &[0]].concat();
        assert_eq!(
            Binary::decode::<HeightGrid>(&long),
            Err(BinaryError::NotCanonical)
        );
        assert_eq!(
            Binary::decode::<HeightGrid>(&trailing),
            Err(BinaryError::NotCanonical)
        );
    }
}
