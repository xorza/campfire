use campfire_common::{Tick, Ticks};
use serde::{Deserialize, Serialize};

use crate::input_rules::InputRules;

/// The last tick a player slot's inputs were scheduled to apply in, and how many apply there.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Spill {
    pub(crate) tick: Tick,
    pub(crate) count: u32,
}

/// The tick an input applies in, as its player slot's spill places it, within the max input lead
/// past the next tick: the one place a spill takes, so no input is queued past the window
/// (05-protocol-spec.md, A late input applies at the next tick).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Admitted(Tick);

impl Spill {
    /// The tick the next input that may apply from tick `earliest` on would apply in: the
    /// first, from there and from the last, that fewer than `max` of the player slot's inputs fill.
    const fn place(self, earliest: Tick, max: u32) -> Tick {
        if self.count == 0 || earliest.get() > self.tick.get() {
            earliest
        } else if self.count < max {
            self.tick
        } else {
            self.tick.after(Ticks::ONE)
        }
    }

    /// The place of the next input that may apply from tick `earliest` on, of a log whose next
    /// tick is `next`; none when it passes `rules`' max lead past `next`.
    pub(crate) fn admit(self, earliest: Tick, next: Tick, rules: &InputRules) -> Option<Admitted> {
        let tick = self.place(earliest, rules.max_per_tick);
        tick.since(next)
            .is_none_or(|lead| lead <= rules.max_lead)
            .then_some(Admitted(tick))
    }

    /// Schedules the input whose place `admit` gave, from this spill, and says its tick.
    pub(crate) fn take(&mut self, place: Admitted) -> Tick {
        let Admitted(tick) = place;
        debug_assert!(
            self.count == 0 || tick.get() >= self.tick.get(),
            "a place comes from this spill's `admit`"
        );
        self.count = if self.count != 0 && tick.get() == self.tick.get() {
            self.count + 1
        } else {
            1
        };
        self.tick = tick;
        tick
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::{Tick, Ticks};

    use crate::input_rules::InputRules;
    use crate::spill::Spill;

    const RULES: InputRules = InputRules {
        max_lead: Ticks::new(2),
        max_payload_len: 8,
        max_per_tick: 2,
    };

    #[test]
    fn a_place_past_the_max_lead_is_not_admitted() {
        let next = Tick::new(10);
        let mut spill = Spill::default();
        // Two inputs a tick from tick 10: 10, 10, 11, 11, 12, 12, then 13 is 3 past the next
        // tick, past the max lead of 2.
        let ticks: Vec<u64> = (0..6)
            .map(|_| spill.take(spill.admit(next, next, &RULES).unwrap()).get())
            .collect();
        assert_eq!(ticks, [10, 10, 11, 11, 12, 12]);
        assert_eq!(spill.admit(next, next, &RULES), None);
        // A tick later the window reaches 13.
        let later = Tick::new(11);
        assert_eq!(
            spill.take(spill.admit(later, later, &RULES).unwrap()),
            Tick::new(13)
        );
    }
}
