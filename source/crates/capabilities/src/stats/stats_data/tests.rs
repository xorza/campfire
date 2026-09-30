use std::str::FromStr;

use super::*;

#[test]
fn a_stat_grows_by_its_per_level_from_level_1() {
    let stats = StatsData(BTreeMap::from([
        (
            Stat::Health,
            StatValue {
                base: Scalar::Int(472),
                per_level: Some(Scalar::Int(84)),
            },
        ),
        (
            Stat::AttackDamage,
            StatValue {
                base: Scalar::Int(47),
                per_level: Some(Scalar::Decimal(Num::from_str("3.8").unwrap())),
            },
        ),
        (
            Stat::Armor,
            StatValue {
                base: Scalar::Int(18),
                per_level: None,
            },
        ),
    ]));
    // 472 + 84 × 2 at level 3; 47 + 3.8 at level 2; no per_level, no growth.
    assert_eq!(stats.at(Stat::Health, 3), Num::from_int(640));
    assert_eq!(stats.at(Stat::AttackDamage, 2), Num::from_str("50.8").ok());
    assert_eq!(stats.at(Stat::Armor, 18), Num::from_int(18));
    assert_eq!(stats.at(Stat::MoveSpeed, 1), None);
    assert_eq!(stats.at(Stat::Health, 0), None);
    assert_eq!(Stat::named("magic_resist"), Some(Stat::MagicResist));
    assert_eq!(Stat::named("magic_resistance"), None);
}
