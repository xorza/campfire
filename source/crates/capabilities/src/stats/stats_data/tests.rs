use std::str::FromStr;

use super::*;
use crate::stats::stat::EngineStat;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_rule::StatRule;
use crate::units::unit_type::UnitType;

#[test]
fn a_stat_grows_by_its_per_level_from_level_1() {
    let value = |base, per_level| StatValue { base, per_level };
    let [health, armor] = ["health", "armor"].map(|name| Stat::named(name).unwrap());
    let attack_damage = Stat::named("attack_damage").unwrap();
    let move_speed = Stat::Engine(EngineStat::MoveSpeed);
    let stats = StatsData(BTreeMap::from([
        (
            health.clone(),
            value(Num::from_int(472).unwrap(), Num::from_int(84).unwrap()),
        ),
        (
            attack_damage.clone(),
            value(Num::from_int(47).unwrap(), Num::from_str("3.8").unwrap()),
        ),
        (armor.clone(), value(Num::from_int(18).unwrap(), Num::ZERO)),
    ]));
    assert!(stats.declares(&health) && !stats.declares(&move_speed));
    // The stat book's ids: move speed, an engine stat, first, then armor, attack damage, health.
    let rules = [&move_speed, &armor, &attack_damage, &health]
        .map(|stat| (stat.clone(), StatRule::default()));
    let unit_type = UnitType::new(0);
    let book = StatBook::new(&BTreeMap::from(rules), [(unit_type, &stats)], Num::MAX);
    // 472 + 84 × 2 at level 3; 47 + 3.8 at level 2; 18 at any level; a stat the type does not
    // give is 0.
    let at =
        |level, stat: &Stat| book.base_values(unit_type, level)[book.named(stat).unwrap().index()];
    assert_eq!(at(3, &health), Num::from_int(640).unwrap());
    assert_eq!(at(2, &attack_damage), Num::from_str("50.8").unwrap());
    assert_eq!(at(7, &armor), Num::from_int(18).unwrap());
    assert_eq!(at(1, &move_speed), Num::ZERO);
    let gives = |stat: &Stat| book.gives(unit_type, book.named(stat).unwrap());
    assert!(gives(&health) && !gives(&move_speed));
}
