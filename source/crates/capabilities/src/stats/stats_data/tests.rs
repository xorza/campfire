use std::str::FromStr;

use super::*;
use crate::stats::stat::EngineStat;

#[test]
fn a_stat_grows_by_its_per_level_from_level_1() {
    let value = |base, per_level| StatValue { base, per_level };
    let [health, armor] = ["health", "armor"].map(|name| Stat::named(name).unwrap());
    let attack_damage = Stat::Engine(EngineStat::AttackDamage);
    let stats = StatsData(BTreeMap::from([
        (
            health.clone(),
            value(Scalar::Int(472), Some(Scalar::Int(84))),
        ),
        (
            attack_damage.clone(),
            value(
                Scalar::Int(47),
                Some(Scalar::Decimal(Num::from_str("3.8").unwrap())),
            ),
        ),
        (armor.clone(), value(Scalar::Int(18), None)),
    ]));
    // 472 + 84 × 2 at level 3; 47 + 3.8 at level 2; 18 at any level; an undeclared stat and
    // level 0 give none.
    let move_speed = Stat::Engine(EngineStat::MoveSpeed);
    assert_eq!(stats.at(&health, 3), Num::from_int(640));
    assert_eq!(stats.at(&attack_damage, 2), Num::from_str("50.8").ok());
    assert_eq!(stats.at(&armor, 7), Num::from_int(18));
    assert_eq!(stats.at(&move_speed, 1), None);
    assert_eq!(stats.at(&health, 0), None);
    assert!(stats.declares(&health) && !stats.declares(&move_speed));
}
