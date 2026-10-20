use std::cell::LazyCell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_capabilities::Release;
use campfire_measure::{Kind, Scale};
use campfire_mode::HostPicks;
use campfire_package_disk::PackageDir;
use campfire_runner::MatchBuild;
use campfire_script::ScriptHost;
use campfire_sim::MatchRate;
use campfire_test_packages::TestMode;
use criterion::Criterion;

use crate::moba_3v3::Moba3v3;

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
    let mut group = Kind::Integration.group(c, "package_load", Scale::Frame);
    group.bench_function("3v3", |b| {
        let (dir, map) = (TestMode::Moba3v3.dir(), TestMode::Moba3v3.map());
        b.iter(|| {
            black_box(
                PackageDir::new(dir.clone())
                    .mode_packages(Some(&map))
                    .expect("the 3v3 loads"),
            )
        });
    });
    group.finish();
    let mut group = Kind::Integration.group(c, "match_build", Scale::Frame);
    group.bench_function("3v3", |b| {
        let packages = moba.packages();
        let rate = MatchRate::new(packages.manifest().tick_hz.default());
        let seats = usize::try_from(Moba3v3::PLAYERS).expect("a player count fits usize");
        let picks = HostPicks::defaults(packages.data(), seats);
        b.iter_custom(|builds| {
            let mut spent = Duration::ZERO;
            for _ in 0..builds {
                let start = Instant::now();
                let game = MatchBuild::restore_target(packages, rate, Moba3v3::PLAYERS, &picks);
                spent += start.elapsed();
                black_box(game);
            }
            spent
        });
    });
    group.finish();
    let mut group = Kind::Integration.group(c, "script_compile", Scale::Frame);
    group.bench_function("3v3", |b| {
        let (packages, per_call) = (moba.packages(), per_call());
        b.iter_custom(|builds| {
            let mut spent = Duration::ZERO;
            for _ in 0..builds {
                let mut host = ScriptHost::new(per_call);
                Release::bind_script_api(&mut host);
                let start = Instant::now();
                for script in packages.scripts() {
                    host.compile(script.source).unwrap();
                }
                spent += start.elapsed();
                black_box(host);
            }
            spent
        });
    });
    group.finish();
    let mut group = Kind::Integration.group(c, "script_bind", Scale::Frame);
    group.bench_function("3v3", |b| {
        let per_call = per_call();
        b.iter_custom(|builds| {
            let mut spent = Duration::ZERO;
            for _ in 0..builds {
                let mut host = ScriptHost::new(per_call);
                let start = Instant::now();
                black_box(Release::bind_script_api(&mut host));
                spent += start.elapsed();
                black_box(host);
            }
            spent
        });
    });
    group.finish();
}
