use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::navigation::broadphase::Broadphase;
use crate::navigation::broadphase::internals::{scene, statics};
use crate::navigation::collider::Collider;
use crate::values::scene::Scene;

/// The Collide stage's work for `Scene::UNITS` bodies, on each of a kernel's scenes: finding the
/// contacts, then parting them, from the same scene every run.
pub(crate) fn collision(c: &mut Criterion) {
    let mut group = c.benchmark_group("collision");
    group.throughput(Throughput::Elements(Scene::UNITS as u64));
    for (name, span) in Scene::DENSITIES {
        let bodies = scene(9, Scene::UNITS, span, 1);
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
