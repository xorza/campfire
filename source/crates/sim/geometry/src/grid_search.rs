use std::cmp::Ordering;
use std::sync::LazyLock;

use campfire_math::{Num, Vec3};

/// The order of every search, the grid's steps within `GridSearch::MAX_STEPS` of the center, by
/// square distance, then by angle: about 51,000 steps, built once, as a match that searches
/// loads, so no search orders its points again and no tick builds it.
static ORDER: LazyLock<Box<[[i16; 2]]>> = LazyLock::new(GridSearch::order);

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

    /// Builds the order of every search now, as a match that searches loads, so no tick's first
    /// search pays for it.
    pub fn prepare() {
        LazyLock::force(&ORDER);
    }

    /// The nearest point of the grid where `passes` holds; none when no point within the bound of
    /// the world does.
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
        let step = |count: i16| {
            i64::try_from(i128::from(count) * gap_bits)
                .ok()
                .map(Num::from_bits)
        };
        ORDER
            .iter()
            .take_while(|&&step| Self::square(step) <= most)
            .find_map(|&[i, j]| {
                let at = step(i)
                    .zip(step(j))
                    .and_then(|(x, z)| position.checked_add(Vec3::new(x, Num::ZERO, z)))?;
                (at.within_bound() && passes(at)).then_some(at)
            })
    }

    /// The grid steps within `MAX_STEPS` of the center, by square distance, then by angle.
    fn order() -> Box<[[i16; 2]]> {
        let reach = i16::try_from(Self::MAX_STEPS).expect("the most steps fit i16");
        let most = Self::square([reach, 0]);
        let mut steps: Vec<[i16; 2]> = (-reach..=reach)
            .flat_map(|i| (-reach..=reach).map(move |j| [i, j]))
            .filter(|&step| Self::square(step) <= most)
            .collect();
        steps.sort_unstable_by(|&a, &b| {
            Self::square(a)
                .cmp(&Self::square(b))
                .then_with(|| Self::by_angle(a.map(i64::from), b.map(i64::from)))
        });
        steps.into_boxed_slice()
    }

    fn square([i, j]: [i16; 2]) -> i64 {
        let [i, j] = [i, j].map(i64::from);
        i * i + j * j
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_visits_the_steps_by_distance_then_by_angle_from_plus_x_toward_minus_z() {
        // Steps of 1 m round (10, 0, 10), within 1 step: the center, then the four at 1 m, from
        // +x through −z, −x and +z. Within 2 steps, the four at √2 m come next, from (+1, −1).
        let visited = |steps| {
            let mut visited = Vec::new();
            let search = GridSearch {
                position: Vec3::new(Num::int(10), Num::ZERO, Num::int(10)),
                gap: Num::ONE,
                steps,
            };
            let found = search.nearest(|at| {
                visited.push([at.x, at.z].map(|axis| axis.to_int().unwrap() - 10));
                false
            });
            assert_eq!(found, None);
            visited
        };
        assert_eq!(visited(1), [[0, 0], [1, 0], [0, -1], [-1, 0], [0, 1]]);
        let two = visited(2);
        assert_eq!(two.len(), 13);
        assert_eq!(&two[5..9], [[1, -1], [-1, -1], [-1, 1], [1, 1]]);
        // The order holds every step of square distance within 128², 51,433 of them, each once.
        assert_eq!(ORDER.len(), 51_433);
        assert!(
            ORDER
                .windows(2)
                .all(|pair| GridSearch::square(pair[0]) <= GridSearch::square(pair[1]))
        );
    }
}
