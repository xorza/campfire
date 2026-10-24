use campfire_actions::ActionProgress;
use campfire_combat::Combat;
use campfire_combat::internals::Armed;
use campfire_declarations::Role;
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_orders::Orders;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, SimWorld};
use campfire_units::internals::Spawn;
use campfire_values::Team;
use criterion::{Criterion, Throughput};

use crate::release::r#match::Match;

/// Idle scanners in each of the two lines.
const SCANNERS: i64 = 256;
/// A scan each 250 ms: at 30 ticks a second, ⌈250 · 30 / 1000⌉ = 8 ticks.
const SCAN_TICKS: i64 = 8;

/// The idle scans of 512 units of a weapon of 2 m, `idle`, a tick of the schedule as a match
/// runs it, with no navigation's walk: two lines of 256, a meter apart within a line and 4 m from
/// the other, so each scan visits the units near it and reaches none, and every unit stays idle,
/// the scan's worst case. Each scans every 8 ticks, staggered by stable id, so a tick scans 64.
pub(super) fn acquire(c: &mut Criterion) {
    let mut lines = None;
    let mut group = Kind::Integration.group(c, "acquire", Scale::Sample);
    group.throughput(Throughput::Elements(
        (2 * SCANNERS / SCAN_TICKS).unsigned_abs(),
    ));
    group.bench_function("idle", |bench| {
        let world = lines.get_or_insert_with(two_lines);
        bench.iter(|| world.step());
    });
    group.finish();
}

/// The two lines of `acquire`, past the 8 ticks in which each unit scanned once.
fn two_lines() -> Match {
    let mut world = Match::builder(&[Capability::Combat, Capability::Orders])
        .role(Role::Rules)
        .build();
    Combat::load_weapon_stats(&mut world);
    let armed = Armed::melee(Num::int(100), Num::int(2), 1, 30, Num::int(10));
    let scanner = armed.load(&mut world);
    Orders::load_acquire(&mut world, scanner, 250, &["enemies"]);
    let mut ids = Vec::new();
    for (team, z) in [(0, 0), (1, 4)] {
        for nth in 0..SCANNERS {
            let at = Vec3::new(Num::int(nth), Num::ZERO, Num::int(z));
            ids.push(
                Spawn::of(scanner, at)
                    .team(Team::new(team))
                    .place(&mut world),
            );
        }
    }
    for _ in 0..SCAN_TICKS {
        world.step();
    }
    let idle = ids
        .iter()
        .all(|&id| world.part::<ActionProgress>(id).attack_target().is_none());
    assert!(idle, "the scene's units stay idle");
    world
}
