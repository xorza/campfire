use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::str::FromStr;

use super::*;
use crate::stats::stats_data::StatValue;

fn decimal(text: &str) -> Num {
    Num::from_str(text).unwrap()
}

fn rules(hz: u32) -> KitRules {
    KitRules {
        rate: TickRate::new(NonZeroU32::new(hz).unwrap()),
        max_move_speed: Speed::new(decimal("6.0")).unwrap(),
        life: Some(PoolId::FIRST),
    }
}

fn health() -> Stat {
    Stat::named("health").unwrap()
}

/// The life pool, the first, its maximum the `health` stat.
fn life(health: &Stat) -> [(PoolId, &Stat); 1] {
    [(PoolId::FIRST, health)]
}

/// The 3v3's caster creep, its stats the pool's and the kit's, with `change` applied.
fn caster(change: impl FnOnce(&mut BTreeMap<Stat, StatValue>)) -> StatsData {
    let base = |text: &str| StatValue {
        base: decimal(text),
        per_level: Num::ZERO,
    };
    let mut stats = BTreeMap::from([
        (health(), base("280")),
        (Stat::Engine(EngineStat::MoveSpeed), base("3.25")),
    ]);
    change(&mut stats);
    StatsData(stats)
}

fn combat(on_death: OnDeath) -> CombatData {
    CombatData { on_death }
}

#[test]
fn a_kit_counts_its_stats_in_ticks_at_the_rate() {
    let health = health();
    let despawn = combat(OnDeath::Despawn);
    let kit = UnitKit::new(
        Some(&caster(|_| ())),
        Some(&despawn),
        life(&health),
        rules(30),
    );
    let kit = kit.unwrap();
    assert_eq!(kit.pools.unwrap().max(PoolId::FIRST), Some(decimal("280")));
    assert_eq!(
        kit.pools.unwrap().current(PoolId::FIRST),
        Some(decimal("280"))
    );
    assert_eq!(kit.on_death, Some(OnDeath::Despawn));
    // 3.25 m/s is 54525952 / 2²⁴: over 30 that is 1817531.73, to 1817532; over 20, 2726297.6,
    // to 2726298.
    assert_eq!(kit.step.unwrap().get(), Num::from_bits(1_817_532));
    let stay = combat(OnDeath::Stay);
    let slower = UnitKit::new(Some(&caster(|_| ())), Some(&stay), life(&health), rules(20));
    let slower = slower.unwrap();
    assert_eq!(slower.step.unwrap().get(), Num::from_bits(2_726_298));
    assert_eq!(slower.on_death, Some(OnDeath::Stay));

    // 7 m/s is over the cap: 6 over 30 is 0.2 m, 3355443.2 / 2²⁴, to 3355443.
    let fast = caster(|stats| {
        stats
            .get_mut(&Stat::Engine(EngineStat::MoveSpeed))
            .unwrap()
            .base = Num::from_int(7).unwrap();
    });
    let kit = UnitKit::new(Some(&fast), None, life(&health), rules(30)).unwrap();
    assert_eq!(kit.step.unwrap().get(), Num::from_bits(3_355_443));

    // No combat section, no move speed, no pools: nothing of any.
    let still = caster(|stats| {
        stats.remove(&Stat::Engine(EngineStat::MoveSpeed));
    });
    let kit = UnitKit::new(Some(&still), None, [], rules(30)).unwrap();
    assert_eq!((kit.on_death, kit.step, kit.pools), (None, None, None));

    // Two pools, each full at its maximum at level 1: health 280, and mana 100 + 20 × 0. A pool
    // the type does not list stays none.
    let mana = Stat::named("mana").unwrap();
    let with_mana = caster(|stats| {
        let value = StatValue {
            base: Num::from_int(100).unwrap(),
            per_level: Num::from_int(20).unwrap(),
        };
        stats.insert(mana.clone(), value);
    });
    let pools = [(PoolId::FIRST, &health), (PoolId::new(2).unwrap(), &mana)];
    let kit = UnitKit::new(Some(&with_mana), None, pools, rules(30)).unwrap();
    let pools = kit.pools.unwrap();
    let maxes = [0, 1, 2].map(|at| pools.max(PoolId::new(at).unwrap()));
    assert_eq!(maxes, [Some(decimal("280")), None, Some(decimal("100"))]);
}

#[test]
fn a_kit_refuses_values_that_make_no_unit() {
    let cases = [
        (
            caster(|stats| {
                stats.remove(&health());
            }),
            UnitKitError::MissingStat(health()),
        ),
        (
            caster(|stats| {
                stats.get_mut(&health()).unwrap().base = Num::from_int(0).unwrap();
            }),
            UnitKitError::NotPositive(health()),
        ),
        (
            caster(|stats| {
                stats
                    .get_mut(&Stat::Engine(EngineStat::MoveSpeed))
                    .unwrap()
                    .base = Num::from_int(-1).unwrap();
            }),
            UnitKitError::Negative(Stat::Engine(EngineStat::MoveSpeed)),
        ),
    ];
    let health = health();
    let despawn = combat(OnDeath::Despawn);
    for (stats, error) in cases {
        assert_eq!(
            UnitKit::new(Some(&stats), Some(&despawn), life(&health), rules(30)),
            Err(error)
        );
    }
    // A type with combat but without the life pool makes no unit.
    let kit = UnitKit::new(Some(&caster(|_| ())), Some(&despawn), [], rules(30));
    assert_eq!(kit, Err(UnitKitError::NoLifePool));
}
