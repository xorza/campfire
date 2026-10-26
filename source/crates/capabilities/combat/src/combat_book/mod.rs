use bevy_ecs::resource::Resource;
use campfire_common::Ticks;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_script::MatchNames;
use campfire_sim::MatchRate;
use campfire_stats::StatBook;
use campfire_units::StatId;
use campfire_values::{PackageIndex, Stat};

use crate::mode_combat_data::ModeCombatData;

/// What the mode's `[combat]` binds besides its life pool: the places among the stats of leech
/// from attacks and from other damage, and of the heal scale, without which its rule does
/// nothing; and how long after its last strike an attacker counts as assisting in a death,
/// `assist_window_ms` in ticks, without which no one assists. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
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

impl CombatBook {
    /// The mode's `[combat]`, each stat it names resolved; an error for a stat the mode does not
    /// declare.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let none = ModeCombatData::default();
        let data = cx.mode_section::<ModeCombatData>().unwrap_or(&none);
        let stats = cx.book::<StatBook>()?;
        let at = || BookPlace::Section(ModeCombatData::KEY);
        Self::new(data, cx.rate(), |stat| {
            stats.named_in(PackageIndex::MODE, at(), stat)
        })
    }
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
mod tests;
