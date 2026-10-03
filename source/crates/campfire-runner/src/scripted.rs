use campfire_capabilities::Order;
use campfire_common::Tick;

use crate::fixed_match::FixedMatch;
use crate::match_units::MatchUnits;

/// One input of a scripted player: what player `slot` orders, sent for `stamp`, as a plan its
/// match resolves to the units that stand when it is sent.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scripted<P> {
    stamp: u64,
    slot: u32,
    plan: P,
}

/// What a scripted input orders, by the role of the units it names.
pub(crate) trait Plan: Copy {
    /// The order this plan of player `slot` stands for, with its units' stable ids in `units`.
    fn order(self, units: MatchUnits<'_>, slot: u32) -> Order;
}

impl<P: Plan> Scripted<P> {
    pub(crate) const fn new(stamp: u64, slot: u32, plan: P) -> Scripted<P> {
        Scripted { stamp, slot, plan }
    }

    /// Runs tick `tick` of `fixed`: first sends the inputs of `script` stamped for it, each in a
    /// packet of its own, then runs it.
    pub(crate) fn play_tick(script: &[Scripted<P>], fixed: &mut FixedMatch, tick: u64) {
        for scripted in script.iter().filter(|scripted| scripted.stamp == tick) {
            let order = scripted.plan.order(MatchUnits::of(fixed), scripted.slot);
            fixed.send(scripted.slot, Tick::new(tick), &Order::payload(&[order]));
        }
        fixed.runner_mut().run_tick();
    }
}
