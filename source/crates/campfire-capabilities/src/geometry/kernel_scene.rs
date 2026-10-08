use campfire_common::SegmentSeed;
use campfire_math::{Num, Rng, RngSource, RngStream, Vec3};

/// A made scene of units for a bench or a test, drawn from a seed, the same in every run: every
/// kernel case draws its units' places from one.
#[derive(Debug)]
pub struct KernelScene {
    rng: Rng,
}

/// How close a kernel case's units stand: crowded into 40 m square, or spread over 120 m square.
#[derive(Debug, Clone, Copy)]
pub enum Density {
    Crowded,
    Spread,
}

impl Density {
    /// Both, for a kernel whose cost grows with how close its units stand.
    pub const ALL: [Density; 2] = [Density::Crowded, Density::Spread];

    /// The case's name.
    pub const fn name(self) -> &'static str {
        match self {
            Density::Crowded => "crowded",
            Density::Spread => "spread",
        }
    }

    /// Half the side of the square the units stand in, in meters.
    pub const fn span(self) -> u64 {
        match self {
            Density::Crowded => 20,
            Density::Spread => 60,
        }
    }
}

impl KernelScene {
    /// The units of a kernel case: the count design 04 sets for an RTS battle.
    pub const UNITS: usize = 1000;

    pub fn new(seed: u64) -> KernelScene {
        let source = RngSource::new(SegmentSeed::new([0; 32]));
        KernelScene {
            rng: source.open(RngStream::new("scene"), seed),
        }
    }

    /// A point on the ground at a whole centimeter within `span` meters of the origin on both
    /// axes.
    pub fn point(&mut self, span: u64) -> Vec3 {
        let mut coordinate = || {
            let cm = self.rng.below(span * 200).cast_signed();
            KernelScene::centimeters(cm - span.cast_signed() * 100)
        };
        let x = coordinate();
        Vec3::new(x, Num::ZERO, coordinate())
    }

    /// A draw below `bound`.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.rng.below(bound)
    }

    /// `cm` centimeters.
    pub(crate) const fn centimeters(cm: i64) -> Num {
        Num::from_bits((cm << Num::FRAC_BITS) / 100)
    }
}

#[cfg(any(test, feature = "bench"))]
mod walls {
    use campfire_math::Num;

    use crate::geometry::kernel_scene::KernelScene;
    use crate::geometry::polygon::Polygon;

    impl KernelScene {
        /// The walls that split a scene of `span` meters' half side into two lanes and a jungle,
        /// as the 3v3's map does: on each side, halfway out, a wall 4 m thick in two pieces,
        /// with a 10 m gap between them at the middle, both ending 10 m short of the scene's
        /// ends, where lanes and jungle meet.
        pub(crate) fn walls(span: u64) -> Vec<Polygon> {
            let span = span.cast_signed();
            let meters = |at: i64| Num::from_int(at).unwrap();
            let mut walls = Vec::with_capacity(4);
            for side in [-1, 1] {
                let (west, east) = (meters(side * span / 2 - 2), meters(side * span / 2 + 2));
                for [south, north] in [[-(span - 10), -5], [5, span - 10]] {
                    let (south, north) = (meters(south), meters(north));
                    let corners = vec![[west, south], [east, south], [east, north], [west, north]];
                    walls.push(Polygon::new(corners).unwrap());
                }
            }
            walls
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::Position;

    use super::*;
    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::navigation::terrain::Terrain;
    use crate::navigation::wall::Wall;
    use crate::units::layer::Layer;

    #[test]
    fn the_walls_split_a_scene_into_two_lanes_and_a_jungle() {
        // Half side 60 m: the walls stand at x from -32 to -28 and from 28 to 32, each from
        // z = -50 to -5 and from 5 to 50, on a grid of 1 m cells over 120 m square.
        let edge = Num::int(60);
        let grid = Grid::new(Num::ONE, Bounds::new([-edge; 2], [edge; 2]).unwrap()).unwrap();
        let walls: Vec<Wall> = KernelScene::walls(60)
            .into_iter()
            .map(|area| Wall {
                layer: Layer::FIRST,
                area,
            })
            .collect();
        let terrain = Terrain::new(&grid, &walls);
        let blocked = terrain.blocked(Layer::FIRST).unwrap();
        let at = |x: i64, z: i64| {
            let pos = Position::new(Vec3::new(
                Num::int(x) + Num::HALF,
                Num::ZERO,
                Num::int(z) + Num::HALF,
            ));
            let cell = grid.cell_of(pos.unwrap()).unwrap();
            blocked[cell / 64] & 1 << (cell % 64) != 0
        };
        // Inside each of the four pieces.
        for (x, z) in [(-30, -20), (-30, 20), (30, -20), (30, 20)] {
            assert!(at(x, z), "({x}, {z}) is walled");
        }
        // Just past each face, the lanes, the jungle, the gaps and the ends where all meet.
        for (x, z) in [
            (-33, -20),
            (-28, -20),
            (27, 20),
            (32, 20),
            (-45, 0),
            (0, 0),
            (45, 0),
            (-30, 0),
            (30, 0),
            (-30, 51),
            (30, -52),
        ] {
            assert!(!at(x, z), "({x}, {z}) is open");
        }
    }
}
