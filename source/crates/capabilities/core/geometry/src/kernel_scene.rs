use campfire_common::{SegmentSeed, Tick};
use campfire_math::{Num, Rng, RngStream, Vec3};

use crate::body_box::BodyBox;
use crate::shape::Shape;

/// A made scene of units for a bench or a test, drawn from a seed, the same in every run: every
/// kernel case draws its units' places from one.
#[derive(Debug)]
pub struct KernelScene {
    rng: Rng,
}

/// A body of a made scene: where it stands, its shape, its layer's index, and whether it may be
/// pushed and walks.
#[derive(Debug, Clone, Copy)]
pub struct SceneBody {
    pub point: Vec3,
    pub shape: Shape,
    pub layer: u8,
    pub movable: bool,
    pub walking: bool,
}

campfire_common::enum_all! {
    /// How close a kernel case's units stand: crowded into 40 m square, or spread over 120 m
    /// square.
    #[derive(Debug, Clone, Copy)]
    pub enum Density {
        Crowded,
        Spread,
    }
    /// Both, for a kernel whose cost grows with how close its units stand.
    pub const ALL;
}

impl Density {
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
        let stream = RngStream::new("scene");
        KernelScene {
            rng: Rng::new(&SegmentSeed::new([0; 32]), stream, seed, Tick::ZERO),
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

    /// `count` bodies: each at a whole centimeter within `span` meters of the origin on both axes,
    /// of a radius from 0.2 to 1.19 m, on one of `layers` layers, and that may be pushed, and
    /// walks, at random. A scene of one layer draws no layer. With `boxes`, a body that may not be
    /// pushed is a box as wide and as deep as its circle, at a whole degree.
    pub fn bodies(&mut self, count: usize, span: u64, layers: u8, boxes: bool) -> Vec<SceneBody> {
        (0..count)
            .map(|_| {
                let point = self.point(span);
                let radius = KernelScene::centimeters(20 + self.below(100).cast_signed());
                let movable = self.below(4) != 0;
                let layer = match layers {
                    1 => 0,
                    _ => u8::try_from(self.below(layers.into())).unwrap(),
                };
                let shape = if boxes && !movable {
                    let angle = Num::from_int(self.below(360).cast_signed()).unwrap();
                    let side = radius + radius;
                    Shape::Box(BodyBox::new([side, side], angle).unwrap())
                } else {
                    Shape::Circle(radius)
                };
                let walking = movable && self.below(2) == 0;
                SceneBody {
                    point,
                    shape,
                    layer,
                    movable,
                    walking,
                }
            })
            .collect()
    }

    /// A draw below `bound`.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.rng.below(bound)
    }

    /// `cm` centimeters.
    pub const fn centimeters(cm: i64) -> Num {
        Num::from_bits((cm << Num::FRAC_BITS) / 100)
    }
}

mod walls {
    use campfire_math::Num;

    use crate::kernel_scene::KernelScene;
    use crate::polygon::Polygon;

    impl KernelScene {
        /// The walls that split a scene of `span` meters' half side into two lanes and a jungle,
        /// as the 3v3's map does: on each side, halfway out, a wall 4 m thick in two pieces,
        /// with a 10 m gap between them at the middle, both ending 10 m short of the scene's
        /// ends, where lanes and jungle meet.
        pub fn walls(span: u64) -> Vec<Polygon> {
            let span = span.cast_signed();
            let meters = |at: i64| Num::from_int(at).unwrap();
            let mut walls = Vec::with_capacity(4);
            for side in [-1, 1] {
                let (west, east) = (meters(side * span / 2 - 2), meters(side * span / 2 + 2));
                for [south, north] in [[-(span - 10), -5], [5, span - 10]] {
                    let (south, north) = (meters(south), meters(north));
                    let corners = [[west, south], [east, south], [east, north], [west, north]];
                    walls.push(Polygon::new(corners).unwrap());
                }
            }
            walls
        }
    }
}
