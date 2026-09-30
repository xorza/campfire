use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::str::FromStr;

use super::*;
use crate::combat::on_death::OnDeath;
use crate::stats::stats_data::StatValue;
use crate::values::scalar::Scalar;

fn decimal(text: &str) -> Num {
    Num::from_str(text).unwrap()
}

fn rules(hz: u32) -> KitRules {
    KitRules {
        rate: TickRate::new(NonZeroU32::new(hz).unwrap()),
        max_move_speed: Speed::new(decimal("6.0")).unwrap(),
    }
}

/// The 3v3's caster creep, with `change` applied to its stats.
fn caster(change: impl FnOnce(&mut BTreeMap<Stat, StatValue>)) -> StatsData {
    let base = |text: &str| StatValue {
        base: Scalar::Decimal(decimal(text)),
        per_level: None,
    };
    let mut stats = BTreeMap::from([
        (Stat::Health, base("280")),
        (Stat::AttackDamage, base("23")),
        (Stat::AttackSpeed, base("0.67")),
        (Stat::MoveSpeed, base("3.25")),
    ]);
    change(&mut stats);
    StatsData(stats)
}

fn attack(windup_ms: u64, projectile_speed: Option<&str>) -> CombatData {
    CombatData {
        attack: Some(AttackData {
            range: Scalar::Decimal(decimal("6.0")),
            windup_ms,
            projectile_speed: projectile_speed.map(|speed| Scalar::Decimal(decimal(speed))),
        }),
        on_death: OnDeath::Despawn,
    }
}

#[test]
fn a_kit_counts_its_stats_in_ticks_at_the_rate() {
    let kit = UnitKit::new(
        Some(&caster(|_| ())),
        Some(&attack(300, Some("6.5"))),
        rules(30),
    );
    let kit = kit.unwrap();
    let combatant = kit.combatant.unwrap();
    assert_eq!(combatant.health.max(), decimal("280"));
    assert_eq!(combatant.on_death, OnDeath::Despawn);
    // 0.67 attacks a second is 11240735 / 2²⁴, to the nearest: 30 × 2²⁴ over that is 44.78,
    // up to 45 ticks. 300 ms is 9 ticks. 6.5 m/s over 30 is 3635063.47 / 2²⁴, to 3635063;
    // 3.25 m/s is 1817531.73 / 2²⁴, to 1817532.
    let stats = combatant.attack.unwrap();
    assert_eq!((stats.period().get(), stats.windup().get()), (45, 9));
    assert_eq!(stats.range(), decimal("6.0"));
    assert_eq!(stats.damage(), decimal("23"));
    assert_eq!(stats.projectile_speed(), Some(Num::from_bits(3_635_063)));
    assert_eq!(kit.step.unwrap().get(), Num::from_bits(1_817_532));

    // At 20 ticks a second: 29.85 up to 30 ticks, 300 ms is 6, and the projectile flies
    // 0.325 m a tick, faster than the cap's 0.3.
    let slower = UnitKit::new(
        Some(&caster(|_| ())),
        Some(&attack(300, Some("6.5"))),
        rules(20),
    );
    let stats = slower.unwrap().combatant.unwrap().attack.unwrap();
    assert_eq!((stats.period().get(), stats.windup().get()), (30, 6));
    assert_eq!(stats.projectile_speed(), Some(decimal("0.325")));

    // 3 attacks a second is capped at 2.5, 12 ticks; 7 m/s at the cap, 6 over 30: 0.2 m.
    let fast = caster(|stats| {
        stats.get_mut(&Stat::AttackSpeed).unwrap().base = Scalar::Int(3);
        stats.get_mut(&Stat::MoveSpeed).unwrap().base = Scalar::Int(7);
    });
    let kit = UnitKit::new(Some(&fast), Some(&attack(0, None)), rules(30)).unwrap();
    assert_eq!(kit.combatant.unwrap().attack.unwrap().period().get(), 12);
    assert_eq!(kit.step.unwrap().get(), Num::from_bits(3_355_443));

    // No combat section, no move speed: nothing of either.
    let still = caster(|stats| {
        stats.remove(&Stat::MoveSpeed);
    });
    let kit = UnitKit::new(Some(&still), None, rules(30)).unwrap();
    assert_eq!((kit.combatant, kit.step), (None, None));
}

#[test]
fn a_kit_refuses_values_that_make_no_unit() {
    let cases = [
        (
            caster(|_| ()),
            attack(300, Some("5.0")),
            UnitKitError::ProjectileNotFaster,
        ),
        // As fast as the cap, in the steps the match takes: not faster.
        (
            caster(|_| ()),
            attack(300, Some("6.0")),
            UnitKitError::ProjectileNotFaster,
        ),
        // 1500 ms is 45 ticks, the period.
        (caster(|_| ()), attack(1500, None), UnitKitError::Attack),
        (
            caster(|stats| {
                stats.remove(&Stat::Health);
            }),
            attack(300, None),
            UnitKitError::MissingStat(Stat::Health),
        ),
        (
            caster(|stats| {
                stats.remove(&Stat::AttackDamage);
            }),
            attack(300, None),
            UnitKitError::MissingStat(Stat::AttackDamage),
        ),
        (
            caster(|stats| stats.get_mut(&Stat::AttackSpeed).unwrap().base = Scalar::Int(0)),
            attack(300, None),
            UnitKitError::NotPositive(Stat::AttackSpeed),
        ),
        (
            caster(|stats| stats.get_mut(&Stat::Health).unwrap().base = Scalar::Int(0)),
            attack(300, None),
            UnitKitError::NotPositive(Stat::Health),
        ),
        (
            caster(|stats| stats.get_mut(&Stat::MoveSpeed).unwrap().base = Scalar::Int(-1)),
            attack(300, None),
            UnitKitError::Negative(Stat::MoveSpeed),
        ),
    ];
    for (stats, combat, error) in cases {
        assert_eq!(
            UnitKit::new(Some(&stats), Some(&combat), rules(30)),
            Err(error)
        );
    }
}
