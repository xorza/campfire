use std::num::NonZeroU32;

use super::*;
use crate::mode_combat_data::LeechData;

#[test]
fn a_book_binds_its_stats_and_counts_its_window_in_ticks() {
    // At 30 ticks a second, 1,000 ms is 30 ticks and 1,010 ms rounds up to 31; `vamp` at
    // stat 2 and `heal` at stat 5, and a stat the book lacks is the error.
    let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
    let stat = |held: &Stat| match held.name() {
        "vamp" => Ok(StatId::new(2)),
        "heal" => Ok(StatId::new(5)),
        _ => Err(held.clone()),
    };
    let named = |name: &str| Some(Stat::named(name).unwrap());
    let rules = |ms, other| ModeCombatData {
        assist_window_ms: Some(ms),
        leech: Some(LeechData {
            attack: None,
            other: named(other),
        }),
        heal_scale: named("heal"),
        ..ModeCombatData::default()
    };
    let book = CombatBook::new(&rules(1_000, "vamp"), rate, stat).unwrap();
    assert_eq!(
        book,
        CombatBook {
            leech_attack: None,
            leech_other: Some(StatId::new(2)),
            heal_scale: Some(StatId::new(5)),
            assist_window: Some(Ticks::new(30)),
        }
    );
    let later = CombatBook::new(&rules(1_010, "vamp"), rate, stat).unwrap();
    assert_eq!(later.assist_window, Some(Ticks::new(31)));
    let lacking = CombatBook::new(&rules(1_000, "drain"), rate, stat);
    assert_eq!(lacking, Err(Stat::named("drain").unwrap()));
}
