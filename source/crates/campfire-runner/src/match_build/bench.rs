use std::cell::LazyCell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::world::World;
use campfire_capabilities::CapabilitySet;
use campfire_common::SegmentSeed;
use campfire_package::ModePackages;
use campfire_script::ScriptHost;
use campfire_sim::{SimUpdate, StateRegistry, TickRate};
use criterion::Criterion;

use crate::harness::moba_3v3::Moba3v3;
use crate::match_build::MatchBuild;

/// The load of the MOBA 3v3's packages from their files, `package_load/3v3`, as a server and each
/// client load them for a session; and the build of a match of them, as a server builds one per
/// session and a verifier one per checkpoint: `match_build/3v3`, the whole build into a world
/// `SimUpdate::prepare` set up; and two of its parts, `script_compile/3v3`, the compile of every
/// script of its packages into a host with the script API bound, which a cache of parsed scripts
/// would save, and `script_bind/3v3`, the bind of the script API into a new host, which shared API
/// modules would save.
pub(crate) fn build(c: &mut Criterion) {
    let moba = LazyCell::new(Moba3v3::load);
    let per_call = || moba.packages().manifest().script_limits.per_call;
    let mut group = c.benchmark_group("integration/package_load");
    group.sample_size(20);
    group.bench_function("3v3", |b| {
        let (dir, map) = (Moba3v3::dir(), Moba3v3::map());
        b.iter(|| black_box(ModePackages::from_dir(&dir, &map).expect("the 3v3 loads")));
    });
    group.finish();
    let mut group = c.benchmark_group("integration/match_build");
    group.sample_size(20);
    group.bench_function("3v3", |b| {
        let packages = moba.packages();
        let rate = TickRate::new(packages.manifest().tick_hz.default());
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
    group.finish();
    let mut group = c.benchmark_group("integration/script_compile");
    group.sample_size(20);
    group.bench_function("3v3", |b| {
        let (packages, per_call) = (moba.packages(), per_call());
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
    group.finish();
    let mut group = c.benchmark_group("integration/script_bind");
    group.sample_size(20);
    group.bench_function("3v3", |b| {
        let per_call = per_call();
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
