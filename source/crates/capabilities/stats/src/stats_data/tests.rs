use std::str::FromStr;

use campfire_script::{NameKind, UnknownName};
use campfire_values::{EngineStat, NameId, UnitType};

use super::*;
use crate::stat_book::StatBook;
use crate::stat_rule::StatRule;

#[test]
fn a_stat_grows_by_its_per_level_from_level_1() {
    let value = |base, per_level| StatValue { base, per_level };
    let [health, armor] = ["health", "armor"].map(|name| Stat::named(name).unwrap());
    let attack_damage = Stat::named("attack_damage").unwrap();
    let move_speed = Stat::Engine(EngineStat::MoveSpeed);
    let stats = StatsData::from([
        (
            health.clone(),
            value(Num::from_int(472).unwrap(), Num::from_int(84).unwrap()),
        ),
        (
            attack_damage.clone(),
            value(Num::from_int(47).unwrap(), Num::from_str("3.8").unwrap()),
        ),
        (armor.clone(), value(Num::from_int(18).unwrap(), Num::ZERO)),
    ]);
    assert!(stats.declares(&health) && !stats.declares(&move_speed));
    // The stat book's ids: move speed, an engine stat, first, then armor, attack damage, health.
    let rules = [&move_speed, &armor, &attack_damage, &health]
        .map(|stat| (stat.clone(), StatRule::default()));
    let unit_type = UnitType::new(0);
    let mut book = StatBook::new(&BTreeMap::from(rules), Num::MAX);
    book.add_type(unit_type, &stats).unwrap();
    // 472 + 84 × 2 at level 3; 47 + 3.8 at level 2; 18 at any level; a stat the type does not
    // give is 0.
    let at =
        |level, stat: &Stat| book.base_values(unit_type, level)[book.named(stat).unwrap().index()];
    assert_eq!(at(3, &health), Num::from_int(640).unwrap());
    assert_eq!(at(2, &attack_damage), Num::from_str("50.8").unwrap());
    assert_eq!(at(7, &armor), Num::from_int(18).unwrap());
    assert_eq!(at(1, &move_speed), Num::ZERO);
    let gives = |unit_type, stat: &Stat| book.gives(unit_type, book.named(stat).unwrap());
    assert!(gives(unit_type, &health) && !gives(unit_type, &move_speed));
    // A type with a stat the book does not hold gets none of its values, its health among them.
    let mana = Stat::named("mana").unwrap();
    let other = UnitType::new(1);
    let mut unknown = stats.clone();
    unknown.0.insert(mana, value(Num::ONE, Num::ZERO));
    assert_eq!(
        book.add_type(other, &unknown),
        Err(UnknownName::new(NameKind::Stat, "mana"))
    );
    let gives = |unit_type, stat: &Stat| book.gives(unit_type, book.named(stat).unwrap());
    assert!(!gives(other, &health) && gives(unit_type, &health));
}
