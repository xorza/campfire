use std::cmp::Ordering;

use campfire_math::{Num, Vec3};

/// A search of the ground grid of `gap` round `position`, within `steps` gaps of it, for the
/// nearest point a test passes: by distance, then by angle from `+x`, as a facing turns, each
/// compared exactly (04-capabilities/navigation.md, A script finds room near a point).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSearch {
    pub position: Vec3,
    pub gap: Num,
    pub steps: u32,
}

impl GridSearch {
    /// The most steps of its gap a search reaches: its points, about `π` times its square, stay
    /// within a script call's work.
    pub const MAX_STEPS: u32 = 128;

    /// The nearest point of the grid where `passes` holds; none when no point within the bound of
    /// the world does. The points are visited in bands of square distance, each twice the last,
    /// so a search that ends near the position orders few.
    pub fn nearest(self, mut passes: impl FnMut(Vec3) -> bool) -> Option<Vec3> {
        let Self {
            position,
            gap,
            steps,
        } = self;
        debug_assert!(
            gap > Num::ZERO && steps <= Self::MAX_STEPS,
            "a search of a positive gap, within its steps"
        );
        let gap_bits = i128::from(gap.to_bits());
        let most = i64::from(steps) * i64::from(steps);
        let mut points = Vec::new();
        let mut low = 0;
        while low <= most {
            let high = (low * 2).max(64).min(most + 1);
            let reach = ((high - 1).cast_unsigned().isqrt()).cast_signed();
            points.clear();
            for i in -reach..=reach {
                for j in -reach..=reach {
                    if (low..high).contains(&(i * i + j * j)) {
                        points.push([i, j]);
                    }
                }
            }
            points.sort_unstable_by(|&a, &b| {
                let square = |[i, j]: [i64; 2]| i * i + j * j;
                square(a).cmp(&square(b)).then_with(|| Self::by_angle(a, b))
            });
            for &[i, j] in &points {
                let step = |count: i64| {
                    i64::try_from(i128::from(count) * gap_bits)
                        .ok()
                        .map(Num::from_bits)
                };
                let at = step(i)
                    .zip(step(j))
                    .and_then(|(x, z)| position.checked_add(Vec3::new(x, Num::ZERO, z)));
                if let Some(at) = at.filter(|at| at.within_bound() && passes(*at)) {
                    return Some(at);
                }
            }
            low = high;
        }
        None
    }

    /// How two grid steps `[x, z]` of one length order by angle from `+x`, as a facing turns,
    /// toward `−z`: the half from `+x` through `−z`, then the other, each by the sign of the
    /// cross product.
    fn by_angle(a: [i64; 2], b: [i64; 2]) -> Ordering {
        let half = |[x, z]: [i64; 2]| u8::from(!(z < 0 || (z == 0 && x > 0)));
        half(a)
            .cmp(&half(b))
            .then_with(|| (a[1] * b[0] - a[0] * b[1]).cmp(&0).reverse())
    }
}
