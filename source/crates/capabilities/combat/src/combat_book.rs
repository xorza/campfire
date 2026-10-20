use bevy_ecs::resource::Resource;
use campfire_common::Ticks;
use campfire_declarations::Book;
use campfire_script::MatchNames;
use campfire_sim::MatchRate;
use campfire_units::StatId;
use campfire_values::Stat;

use crate::mode_combat_data::ModeCombatData;

/// What the mode's `[combat]` binds besides its life pool: the places among the stats of leech
/// from attacks and from other damage, and of the heal scale, without which its rule does
/// nothing; and how long after its last strike an attacker counts as assisting in a death,
/// `assist_window_ms` in ticks, without which no one assists. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CombatBook {
    pub(crate) leech_attack: Option<StatId>,
    pub(crate) leech_other: Option<StatId>,
    pub(crate) heal_scale: Option<StatId>,
    pub(crate) assist_window: Option<Ticks>,
}

impl CombatBook {
    /// The book of `rules` at `rate`, each stat resolved by `stat`; the first error `stat`
    /// gives. A window past what ticks can count covers the whole match.
    pub fn new<E>(
        rules: &ModeCombatData,
        rate: MatchRate,
        stat: impl Fn(&Stat) -> Result<StatId, E>,
    ) -> Result<Self, E> {
        let leech = rules.leech.as_ref();
        let bound = |held: Option<&Stat>| held.map(&stat).transpose();
        Ok(Self {
            leech_attack: bound(leech.and_then(|leech| leech.attack.as_ref()))?,
            leech_other: bound(leech.and_then(|leech| leech.other.as_ref()))?,
            heal_scale: bound(rules.heal_scale.as_ref())?,
            assist_window: rules.assist_window_ms.map(|ms| rate.window(ms)),
        })
    }
}

impl Book for CombatBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_common::Ticks;

    use crate::combat_book::CombatBook;

    impl CombatBook {
        /// Sets the assist window, as a mode's `assist_window_ms` would.
        pub const fn set_assist_window(&mut self, window: Option<Ticks>) {
            self.assist_window = window;
        }
    }
}

#[cfg(test)]
mod tests {
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
}
