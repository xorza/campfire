use std::str::FromStr;

use super::*;

#[test]
fn an_engine_stat_grows_by_its_per_level_from_level_1() {
    let value = |base, per_level| StatValue { base, per_level };
    let armor = Stat::named("armor").unwrap();
    let stats = StatsData(BTreeMap::from([
        (
            Stat::Engine(EngineStat::Health),
            value(Scalar::Int(472), Some(Scalar::Int(84))),
        ),
        (
            Stat::Engine(EngineStat::AttackDamage),
            value(
                Scalar::Int(47),
                Some(Scalar::Decimal(Num::from_str("3.8").unwrap())),
            ),
        ),
        (armor.clone(), value(Scalar::Int(18), None)),
    ]));
    // 472 + 84 × 2 at level 3; 47 + 3.8 at level 2; an undeclared stat and level 0 give none.
    assert_eq!(stats.at(EngineStat::Health, 3), Num::from_int(640));
    assert_eq!(
        stats.at(EngineStat::AttackDamage, 2),
        Num::from_str("50.8").ok()
    );
    assert_eq!(stats.at(EngineStat::MoveSpeed, 1), None);
    assert_eq!(stats.at(EngineStat::Health, 0), None);
    assert!(stats.declares(EngineStat::Health) && !stats.declares(EngineStat::MoveSpeed));
    // A stat the mode declares stays in the data, for its scripts and modifiers.
    assert_eq!(stats.0[&armor].base, Scalar::Int(18));
}
