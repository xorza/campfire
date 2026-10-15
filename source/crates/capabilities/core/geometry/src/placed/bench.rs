use std::hint::black_box;

use campfire_math::{Num, Vec3};
use criterion::{Criterion, Throughput};

use crate::body_box::BodyBox;
use crate::body_box::bench::COUNT;
use crate::kernel_scene::KernelScene;
use crate::placed::Placed;
use crate::radius::Radius;
use crate::shape::Shape;

/// A body, and a path from one point to another.
#[derive(Debug, Clone, Copy)]
struct Beside {
    body: Placed,
    from: Vec3,
    to: Vec3,
}

/// `COUNT` bodies at whole centimeters within 20 m of the origin, each a circle of 0.2 to 5 m,
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
                Shape::Circle(Radius::new(length(20, 500)).unwrap())
            };
            let pos = scene.point(20);
            let from = scene.point(20);
            let to = from + scene.point(5);
            Beside {
                body: Placed::new(pos, shape),
                from,
                to,
            }
        })
        .collect()
}

/// `Placed::comes_closer` over `COUNT` inputs, as a walker's way tests each body near it, with a
/// reach of 0.5 m: a path against a circle, `comes_closer_circle`, and against a box,
/// `comes_closer_box`.
pub(crate) fn placed(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/placed");
    group.throughput(Throughput::Elements(COUNT as u64));
    let reach = Num::HALF;
    for (case, boxes) in [("comes_closer_circle", false), ("comes_closer_box", true)] {
        let inputs = beside(3 + u64::from(boxes), boxes);
        group.bench_function(case, |bench| {
            bench.iter(|| {
                for input in &inputs {
                    let near = input
                        .body
                        .comes_closer(black_box(input.from), input.to, reach);
                    black_box(near);
                }
            });
        });
    }
    group.finish();
}
