use campfire_combat::Combat;
use campfire_combat::internals::Armed;
use campfire_declarations::Role;
use campfire_geometry::internals::KernelScene;
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_orders::Orders;
use campfire_sim::Capability;
use campfire_units::internals::Spawn;
use campfire_values::Team;
use criterion::Criterion;

use crate::bench::run_cases;
use crate::release::r#match::Match;

/// The units of each line.
const LINE: usize = KernelScene::UNITS / 2;
/// The ticks of a run: each unit takes 10 strikes to fall, one each second, so the first foes fall
/// by tick 300, and those that found a next foe fight on until tick 400, when the rest stand with no
/// foe in reach.
const TICKS: u64 = 400;

/// A melee battle of two lines of `LINE` units of teams 0 and 1, each unit a meter from the next
/// and 1.5 m from the other line, so each finds a foe within its weapon's 2 m at its first scan:
/// each of 100 life, striking 10 a second after a windup of a third of one, scanning for foes each
/// 250 ms, and gone as it falls, so a unit whose foe fell finds the next. A tick of the match's
/// schedule as it runs a battle of the size of a large Zero Hour one, with no navigation's walk,
/// as `run_cases` times it.
pub(super) fn combat(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "combat", Scale::Frame);
    run_cases(&mut group, "melee", TICKS, two_lines);
    group.finish();
}

/// The two lines of `combat`, before their first tick.
fn two_lines() -> Match {
    let mut world = Match::builder(&[Capability::Combat, Capability::Orders])
        .role(Role::Rules)
        .build();
    Combat::load_weapon_stats(&mut world);
    let armed = Armed::melee(Num::int(100), Num::int(2), 10, 30, Num::int(10));
    let fighter = armed.load(&mut world);
    Orders::load_acquire(&mut world, fighter, 250, &["enemies"]);
    let half = Num::int(3) / 4;
    for (team, z) in [(0, -half), (1, half)] {
        for nth in (0..).take(LINE) {
            let at = Vec3::new(Num::int(nth), Num::ZERO, z);
            Spawn::of(fighter, at)
                .team(Team::new(team))
                .place(&mut world);
        }
    }
    world
}
