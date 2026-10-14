use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::str::FromStr;

use campfire_math::Num;
use campfire_stats::{StatRule, StatValue, StatsData};

use super::*;

fn decimal(text: &str) -> Num {
    Num::from_str(text).unwrap()
}

fn health() -> Stat {
    Stat::named("health").unwrap()
}

fn mana() -> Stat {
    Stat::named("mana").unwrap()
}

const MOVE_SPEED: Stat = Stat::Engine(EngineStat::MoveSpeed);
const TURN_RATE: Stat = Stat::Engine(EngineStat::TurnRate);

/// The one unit type the tests' books know.
const TYPE: UnitType = UnitType::new(0);

/// The life pool, the first, its maximum the `health` stat.
fn life(health: &Stat) -> [(PoolId, &Stat); 1] {
    [(PoolId::FIRST, health)]
}

/// The 3v3's caster creep, its stats the pool's and the start's, with `change` applied.
fn caster(change: impl FnOnce(&mut BTreeMap<Stat, StatValue>)) -> StatsData {
    let base = |text: &str| StatValue {
        base: decimal(text),
        per_level: Num::ZERO,
    };
    let mut stats = BTreeMap::from([(health(), base("280")), (MOVE_SPEED, base("3.25"))]);
    change(&mut stats);
    StatsData(stats)
}

/// The start of a type of `stats`, with `combat` and `pools`, at `hz` ticks a second under a cap
/// of 6 m/s, health within `health_rule`.
fn start_of<'a>(
    stats: &StatsData,
    combat: Option<&'a CombatData>,
    pools: &[(PoolId, &'a Stat)],
    hz: u32,
    health_rule: StatRule,
) -> Result<UnitStart, UnitStartError> {
    let rules = BTreeMap::from([
        (health(), health_rule),
        (mana(), StatRule::default()),
        (MOVE_SPEED, StatRule::default()),
        (TURN_RATE, StatRule::default()),
    ]);
    let rate = TickRate::new(NonZeroU32::new(hz).unwrap());
    let mut book = StatBook::new(&rules, decimal("6.0"));
    book.add_type(TYPE, stats).unwrap();
    UnitStart::new(StartOf {
        book: &book,
        life: Some(PoolId::FIRST),
        rate,
        unit_type: TYPE,
        combat,
        pools,
    })
}

fn start<'a>(
    stats: &StatsData,
    combat: Option<&'a CombatData>,
    pools: &[(PoolId, &'a Stat)],
    hz: u32,
) -> Result<UnitStart, UnitStartError> {
    start_of(stats, combat, pools, hz, StatRule::default())
}

fn combat(on_death: OnDeath) -> CombatData {
    CombatData { on_death }
}

#[test]
fn a_start_takes_its_values_at_level_1_from_the_stat_book() {
    let health = health();
    let despawn = combat(OnDeath::Despawn);
    let unit = start(&caster(|_| ()), Some(&despawn), &life(&health), 30).unwrap();
    let full = unit.life.unwrap();
    assert_eq!(
        (full.max(), full.current()),
        (decimal("280"), decimal("280"))
    );
    assert_eq!(unit.pools, None);
    assert_eq!(unit.on_death, Some(OnDeath::Despawn));
    // 3.25 m/s is 54525952 / 2²⁴: over 30 that is 1817531.73, to 1817532; over 20, 2726297.6,
    // to 2726298.
    assert_eq!(unit.step.unwrap().get(), Num::from_bits(1_817_532));
    let stay = combat(OnDeath::Stay);
    let slower = start(&caster(|_| ()), Some(&stay), &life(&health), 20).unwrap();
    assert_eq!(slower.step.unwrap().get(), Num::from_bits(2_726_298));
    assert_eq!(slower.on_death, Some(OnDeath::Stay));

    // 7 m/s is over the cap: 6 over 30 is 0.2 m, 3355443.2 / 2²⁴, to 3355443. A negative
    // speed walks no step, as the book keeps a speed from 0 to the cap.
    let speed = |base: i64| {
        caster(|stats| stats.get_mut(&MOVE_SPEED).unwrap().base = Num::from_int(base).unwrap())
    };
    let fast = start(&speed(7), Some(&stay), &life(&health), 30).unwrap();
    assert_eq!(fast.step.unwrap().get(), Num::from_bits(3_355_443));
    let backward = start(&speed(-1), Some(&stay), &life(&health), 30).unwrap();
    assert_eq!(backward.step.unwrap().get(), Num::ZERO);

    // A turn rate of 90° a second turns 3° a tick at 30 ticks a second and 4.5° at 20; a negative
    // one turns none. A type that turns but does not walk has no turn step, nor one of no turn
    // rate.
    let turning = |rate: &str| {
        caster(|stats| {
            let value = StatValue {
                base: decimal(rate),
                per_level: Num::ZERO,
            };
            stats.insert(TURN_RATE, value);
        })
    };
    let turn = |stats: &StatsData, hz| {
        let unit = start(stats, Some(&stay), &life(&health), hz).unwrap();
        unit.turn.map(TurnStep::get)
    };
    assert_eq!(turn(&turning("90"), 30), Some(Num::int(3)));
    assert_eq!(turn(&turning("90"), 20), Some(decimal("4.5")));
    assert_eq!(turn(&turning("-5"), 30), Some(Num::ZERO));
    assert_eq!(turn(&caster(|_| ()), 30), None);
    let mut rooted = turning("90");
    rooted.0.remove(&MOVE_SPEED);
    assert_eq!(turn(&rooted, 30), None);

    // No combat section, no move speed, no pools: nothing of any.
    let still = caster(|stats| {
        stats.remove(&MOVE_SPEED);
    });
    let unit = start(&still, None, &[], 30).unwrap();
    assert_eq!((unit.on_death, unit.step, unit.pools), (None, None, None));
    assert_eq!(unit.life, None);
    // Nor in a mode that declares no move speed among its stats, as a mode with no stats does.
    let rules = BTreeMap::from([(health.clone(), StatRule::default())]);
    let mut book = StatBook::new(&rules, decimal("6.0"));
    book.add_type(TYPE, &still).unwrap();
    let rate = TickRate::new(NonZeroU32::new(30).unwrap());
    let of = StartOf {
        book: &book,
        life: Some(PoolId::FIRST),
        rate,
        unit_type: TYPE,
        combat: None,
        pools: &[],
    };
    let unit = UnitStart::new(of).unwrap();
    assert_eq!(unit.step, None);

    // Two pools, each full at its maximum at level 1: health 280, the life, and mana 100 + 20 ×
    // 0, which the pools hold. A pool the type does not list stays none, and the pools hold
    // none in the life pool's place.
    let mana = mana();
    let with_mana = caster(|stats| {
        let value = StatValue {
            base: Num::from_int(100).unwrap(),
            per_level: Num::from_int(20).unwrap(),
        };
        stats.insert(mana.clone(), value);
    });
    let pools = [(PoolId::FIRST, &health), (PoolId::new(2).unwrap(), &mana)];
    let unit = start(&with_mana, Some(&stay), &pools, 30).unwrap();
    let pools = unit.pools.unwrap();
    let maxes = [0, 1, 2].map(|at| pools.max(PoolId::new(at).unwrap()));
    assert_eq!(maxes, [None, None, Some(decimal("100"))]);
    assert_eq!(unit.life.map(Life::max), Some(decimal("280")));

    // The mode's limit on health holds the pool's maximum, 280, at 200, as a refresh does.
    let rule = StatRule {
        min: None,
        max: Some(decimal("200")),
    };
    let unit = start_of(&caster(|_| ()), Some(&stay), &life(&health), 30, rule).unwrap();
    assert_eq!(unit.life.map(Life::max), Some(decimal("200")));
}

#[test]
fn a_start_refuses_values_that_make_no_unit() {
    let cases = [
        (
            caster(|stats| {
                stats.remove(&health());
            }),
            UnitStartError::MissingStat(health()),
        ),
        (
            caster(|stats| {
                stats.get_mut(&health()).unwrap().base = Num::from_int(0).unwrap();
            }),
            UnitStartError::NotPositive(health()),
        ),
    ];
    let health = health();
    let despawn = combat(OnDeath::Despawn);
    for (stats, error) in cases {
        assert_eq!(
            start(&stats, Some(&despawn), &life(&health), 30),
            Err(error)
        );
    }
    // A type with combat but without the life pool makes no unit, and nor does one with the life
    // pool but without combat, which would die and never be dead.
    let no_life = start(&caster(|_| ()), Some(&despawn), &[], 30);
    assert_eq!(no_life, Err(UnitStartError::NoLifePool));
    let no_combat = start(&caster(|_| ()), None, &life(&health), 30);
    assert_eq!(no_combat, Err(UnitStartError::NoCombat));
}
