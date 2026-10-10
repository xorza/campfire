use std::hint::black_box;

use campfire_math::Num;
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::geometry::body_box::BodyBox;
use crate::geometry::body_box::bench::COUNT;
use crate::geometry::kernel_scene::KernelScene;
use crate::geometry::shape::Shape;

/// A shape at its point, and a path from one point to another.
#[derive(Debug, Clone, Copy)]
struct Beside {
    shape: Shape,
    at: Position,
    from: Position,
    to: Position,
}

/// `COUNT` shapes at whole centimeters within 20 m of the origin, each a circle of 0.2 to 5 m,
/// or with `boxes` a box of sides 1 to 9 m at a whole degree, with a path beside each from a
/// point within 20 m of the origin to one within 5 m of that.
fn beside(seed: u64, boxes: bool) -> Vec<Beside> {
    let mut scene = KernelScene::new(seed);
    (0..COUNT)
        .map(|_| {
            let mut length = |from: u64, to: u64| {
                let cm = from + scene.below(to - from + 1);
                KernelScene::centimeters(cm.cast_signed())
            };
            let shape = if boxes {
                let size = [length(100, 900), length(100, 900)];
                let angle = Num::from_int(scene.below(360).cast_signed()).unwrap();
                Shape::Box(BodyBox::new(size, angle).unwrap())
            } else {
                Shape::Circle(length(20, 500))
            };
            let at = Position::new(scene.point(20)).unwrap();
            let from = scene.point(20);
            let to = from + scene.point(5);
            Beside {
                shape,
                at,
                from: Position::new(from).unwrap(),
                to: Position::new(to).unwrap(),
            }
        })
        .collect()
}

/// `Shape::comes_within` over `COUNT` inputs, as a walker's way tests each body near it, with a
/// reach of 0.5 m: a path against a circle, `comes_within_circle`, and against a box,
/// `comes_within_box`.
pub(crate) fn shape(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/shape");
    group.throughput(Throughput::Elements(COUNT as u64));
    let reach = Num::HALF;
    for (case, boxes) in [("comes_within_circle", false), ("comes_within_box", true)] {
        let inputs = beside(3 + u64::from(boxes), boxes);
        group.bench_function(case, |bench| {
            bench.iter(|| {
                for input in &inputs {
                    let near =
                        input
                            .shape
                            .comes_within(input.at, black_box(input.from), input.to, reach);
                    black_box(near);
                }
            });
        });
    }
    group.finish();
}
