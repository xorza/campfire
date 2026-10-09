use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::world::World;
use campfire_capabilities::CapabilitySet;
use campfire_common::SegmentSeed;
use campfire_script::ScriptHost;
use campfire_sim::{SimUpdate, StateRegistry, TickRate};
use criterion::Criterion;

use crate::harness::moba_3v3::Moba3v3;
use crate::match_build::MatchBuild;

/// The build of a match of the MOBA 3v3, as a server builds one per session and a verifier
/// one per checkpoint: `build_3v3`, the whole build into a world `SimUpdate::prepare` set up;
/// `parse_3v3`, the compile of every script of its packages into a host with the script API
/// bound, the part of a build that a cache of parsed scripts would save; and `bind_3v3`, the
/// bind of the script API into a new host, the part that shared API modules would save.
pub(crate) fn build(c: &mut Criterion) {
    let moba = Moba3v3::load();
    let packages = moba.packages();
    let per_call = packages.manifest().script_limits.per_call;
    let rate = TickRate::new(packages.manifest().tick_hz.default());
    let mut group = c.benchmark_group("match_build");
    group.sample_size(20);
    group.bench_function("build_3v3", |b| {
        b.iter_custom(|builds| {
            let mut spent = Duration::ZERO;
            for _ in 0..builds {
                let mut world = World::new();
                SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
                let mut schedule = SimUpdate::schedule();
                let mut registry = StateRegistry::new();
                let start = Instant::now();
                MatchBuild::run(
                    packages,
                    &mut world,
                    &mut schedule,
                    &mut registry,
                    Moba3v3::PLAYERS,
                );
                spent += start.elapsed();
                black_box((world, schedule, registry));
            }
            spent
        });
    });
    group.bench_function("parse_3v3", |b| {
        b.iter_custom(|builds| {
            let mut spent = Duration::ZERO;
            for _ in 0..builds {
                let mut host = ScriptHost::new(per_call);
                CapabilitySet::bind_script_api(&mut host);
                let start = Instant::now();
                packages.compile_scripts(|source| host.compile(source));
                spent += start.elapsed();
                black_box(host);
            }
            spent
        });
    });
    group.bench_function("bind_3v3", |b| {
        b.iter_custom(|builds| {
            let mut spent = Duration::ZERO;
            for _ in 0..builds {
                let mut host = ScriptHost::new(per_call);
                let start = Instant::now();
                black_box(CapabilitySet::bind_script_api(&mut host));
                spent += start.elapsed();
                black_box(host);
            }
            spent
        });
    });
    group.finish();
}
