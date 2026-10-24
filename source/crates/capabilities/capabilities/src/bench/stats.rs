use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::Role;
use campfire_geometry::internals::KernelScene;
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_navigation::{Navigation, Walker};
use campfire_sim::Capability;
use campfire_sim::internals::SimWorldInternals;
use campfire_stats::{
    AuraData, CarriedMut, ModifierData, Pools, StatChange, StatOp, StatValue, Stats,
};
use campfire_units::internals::Spawn;
use campfire_units::{Layer, PoolId, Units};
use campfire_values::{EngineStat, FilterData, Number, PackageIndex, Scalar, Stat, Team};
use criterion::Criterion;

use crate::bench::run_cases;
use crate::fixtures::stats::StatsFixtures;
use crate::fixtures::walker_spawn::WalkerSpawn;
use crate::release::r#match::Match;

/// The carriers of an aura: a grid of 10 by 5.
const CARRIERS: usize = 50;
/// The ticks of a run: the walkers' 100 m at 5 m/s, a sixth of a meter a tick at 30 ticks a
/// second, take 600.
const TICKS: u64 = 600;
/// The radius of each aura, in meters.
const RADIUS: i64 = 6;

/// `KernelScene::UNITS` units of team 0: `CARRIERS` that stand 16 m apart, each carrying an aura
/// of `RADIUS` m on its allies that adds 5 to their attack damage, and the rest walking at 5 m/s,
/// in 38 rows of 25 1.5 m apart, each to the point 100 m east of it, through the auras, so each
/// tick some units enter an aura and some leave one, and their stats derive again. A tick of the
/// match's schedule, as `run_cases` times it.
pub(super) fn stats(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "stats", Scale::Frame);
    run_cases(&mut group, "auras", TICKS, auras);
    group.finish();
}

/// The units of `stats`, before their first tick.
fn auras() -> Match {
    let mut world = Match::builder(&[Capability::Stats, Capability::Navigation])
        .role(Role::Rules)
        .build();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::HALF,
    };
    Navigation::load_open_pathing(&mut world, Num::HALF, [-80, -80], [80, 80], [walker]);
    let speed = StatValue {
        base: Num::int(5),
        per_level: Num::ZERO,
    };
    let damage = StatValue {
        base: Num::int(10),
        per_level: Num::ZERO,
    };
    let attack_damage = Stat::named("attack_damage").unwrap();
    let types = [[
        (Stat::Engine(EngineStat::MoveSpeed), speed),
        (attack_damage.clone(), damage),
    ]
    .into_iter()
    .collect()];
    let unit_type = Stats::load_typed_book(&mut world, &types)[0];
    // The book's pools, `health` and `mana`, as its stats derive their maximums.
    let pools = Pools::new([PoolId::FIRST, PoolId::new(1).unwrap()].map(|pool| (pool, Num::ONE)));
    Units::give_type(&mut world, unit_type, pools.unwrap());
    let inspired = ModifierData {
        stats: BTreeMap::from([(
            attack_damage,
            StatChange {
                op: StatOp::Add,
                value: Number::Value(Scalar::Int(5)),
            },
        )]),
        ..Stats::modifier_data(None)
    };
    Stats::load_modifier(&mut world, PackageIndex::MODE, "inspired", &inspired, None);
    let presence = Stats::modifier_data(Some(AuraData {
        radius: Number::Value(Scalar::Int(RADIUS)),
        affects: FilterData::parse("allies").unwrap(),
        modifier: DeclaredName::new("inspired").unwrap(),
    }));
    Stats::load_modifier(&mut world, PackageIndex::MODE, "presence", &presence, None);
    let presence = Stats::modifier(&world, PackageIndex::MODE, "presence").unwrap();
    let at = |x: i64, z: i64| Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
    let team = Team::new(0);
    for nth in (0..).take(CARRIERS) {
        let carrier = Spawn::of(unit_type, at(16 * (nth % 10) - 72, 16 * (nth / 10) - 32))
            .team(team)
            .place(&mut world);
        let entity = world.unit_entity(carrier);
        let aura = Stats::aura(presence, carrier, Num::int(RADIUS));
        CarriedMut::with(&mut world, entity, |carried| carried.apply(aura)).unwrap();
    }
    for nth in (0..).take(KernelScene::UNITS - CARRIERS) {
        let from = Vec3::new(
            Num::int(3 * (nth % 25)) / 2 - Num::int(78),
            Num::ZERO,
            Num::int(2 * (nth / 25) - 37),
        );
        WalkerSpawn::new(Spawn::of(unit_type, from))
            .team(team)
            .circle(Num::HALF)
            .walks(Num::ONE / 6, Some(from + Vec3::from((100, 0, 0))))
            .place(&mut world);
    }
    world
}
