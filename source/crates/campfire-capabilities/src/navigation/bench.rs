use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::navigation::broadphase::Broadphase;
use crate::navigation::broadphase::internals::{scene, statics};
use crate::navigation::collider::Collider;
use crate::values::kernel_scene::KernelScene;

/// The Collide stage's work for `KernelScene::UNITS` bodies, on each of a kernel's scenes: finding the
/// contacts, then parting them, from the same scene every run.
pub(crate) fn collision(c: &mut Criterion) {
    let mut group = c.benchmark_group("collision");
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    for (name, span) in KernelScene::DENSITIES {
        let bodies = scene(9, KernelScene::UNITS, span, 1);
        let index = statics(&bodies);
        let mut broadphase = Broadphase::default();
        let mut colliders = bodies.clone();
        group.bench_function(name, |bench| {
            bench.iter(|| {
                colliders.clone_from(&bodies);
                let contacts = broadphase.contacts(black_box(&colliders), &index);
                Collider::resolve(&mut colliders, contacts);
                black_box(&colliders);
            });
        });
    }
    group.finish();
}
