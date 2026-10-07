use std::collections::BTreeMap;
use std::str::FromStr;

use super::*;
use crate::stats::stat_rule::StatRule;
use crate::stats::stats_data::{StatValue, StatsData};

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

/// The one unit type the tests' books know.
const TYPE: UnitType = UnitType::new(0);

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
    let mut stats = BTreeMap::from([(health(), base("280")), (MOVE_SPEED, base("3.25"))]);
    change(&mut stats);
    StatsData(stats)
}

/// The kit of a type of `stats`, with `combat` and `pools`, at `hz` ticks a second under a cap of
/// 6 m/s, health within `health_rule`.
fn kit_of<'a>(
    stats: &StatsData,
    combat: Option<&'a CombatData>,
    pools: impl IntoIterator<Item = (PoolId, &'a Stat)>,
    hz: u32,
    health_rule: StatRule,
) -> Result<UnitKit, UnitKitError> {
    let rules = BTreeMap::from([
        (health(), health_rule),
        (mana(), StatRule::default()),
        (MOVE_SPEED, StatRule::default()),
    ]);
    let rate = TickRate::new(NonZeroU32::new(hz).unwrap());
    let book = StatBook::new(&rules, [(TYPE, stats)], decimal("6.0"));
    let sections = KitSections {
        combat,
        pools,
        vision: None,
        body: None,
        tracks: TrackSet::default(),
        production: None,
        builds: false,
        node: None,
        gathers: false,
        inventory: None,
    };
    UnitKit::new(&book, TYPE, sections, Some(PoolId::FIRST), rate)
}

fn kit<'a>(
    stats: &StatsData,
    combat: Option<&'a CombatData>,
    pools: impl IntoIterator<Item = (PoolId, &'a Stat)>,
    hz: u32,
) -> Result<UnitKit, UnitKitError> {
    kit_of(stats, combat, pools, hz, StatRule::default())
}

fn combat(on_death: OnDeath) -> CombatData {
    CombatData { on_death }
}

#[test]
fn a_kit_takes_its_values_at_level_1_from_the_stat_book() {
    let health = health();
    let despawn = combat(OnDeath::Despawn);
    let kit = kit(&caster(|_| ()), Some(&despawn), life(&health), 30).unwrap();
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
    let slower = self::kit(&caster(|_| ()), Some(&stay), life(&health), 20).unwrap();
    assert_eq!(slower.step.unwrap().get(), Num::from_bits(2_726_298));
    assert_eq!(slower.on_death, Some(OnDeath::Stay));

    // 7 m/s is over the cap: 6 over 30 is 0.2 m, 3355443.2 / 2²⁴, to 3355443. A negative
    // speed walks no step, as the book keeps a speed from 0 to the cap.
    let speed = |base: i64| {
        caster(|stats| stats.get_mut(&MOVE_SPEED).unwrap().base = Num::from_int(base).unwrap())
    };
    let fast = self::kit(&speed(7), Some(&stay), life(&health), 30).unwrap();
    assert_eq!(fast.step.unwrap().get(), Num::from_bits(3_355_443));
    let backward = self::kit(&speed(-1), Some(&stay), life(&health), 30).unwrap();
    assert_eq!(backward.step.unwrap().get(), Num::ZERO);

    // No combat section, no move speed, no pools: nothing of any.
    let still = caster(|stats| {
        stats.remove(&MOVE_SPEED);
    });
    let kit = self::kit(&still, None, [], 30).unwrap();
    assert_eq!((kit.on_death, kit.step, kit.pools), (None, None, None));

    // Two pools, each full at its maximum at level 1: health 280, and mana 100 + 20 × 0. A pool
    // the type does not list stays none.
    let mana = mana();
    let with_mana = caster(|stats| {
        let value = StatValue {
            base: Num::from_int(100).unwrap(),
            per_level: Num::from_int(20).unwrap(),
        };
        stats.insert(mana.clone(), value);
    });
    let pools = [(PoolId::FIRST, &health), (PoolId::new(2).unwrap(), &mana)];
    let kit = self::kit(&with_mana, Some(&stay), pools, 30).unwrap();
    let pools = kit.pools.unwrap();
    let maxes = [0, 1, 2].map(|at| pools.max(PoolId::new(at).unwrap()));
    assert_eq!(maxes, [Some(decimal("280")), None, Some(decimal("100"))]);

    // The mode's limit on health holds the pool's maximum, 280, at 200, as a refresh does.
    let rule = StatRule {
        min: None,
        max: Some(decimal("200")),
    };
    let kit = kit_of(&caster(|_| ()), Some(&stay), life(&health), 30, rule).unwrap();
    assert_eq!(kit.pools.unwrap().max(PoolId::FIRST), Some(decimal("200")));
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
    ];
    let health = health();
    let despawn = combat(OnDeath::Despawn);
    for (stats, error) in cases {
        assert_eq!(kit(&stats, Some(&despawn), life(&health), 30), Err(error));
    }
    // A type with combat but without the life pool makes no unit, and nor does one with the life
    // pool but without combat, which would die and never be dead.
    let no_life = kit(&caster(|_| ()), Some(&despawn), [], 30);
    assert_eq!(no_life, Err(UnitKitError::NoLifePool));
    let no_combat = kit(&caster(|_| ()), None, life(&health), 30);
    assert_eq!(no_combat, Err(UnitKitError::NoCombat));
}
