use std::num::NonZeroU32;

use campfire_capabilities::{ActionTarget, Order};
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_sim::Position;

use crate::harness::fixed_match::FixedMatch;
use crate::harness::match_units::MatchUnits;

/// The tick rate of the scripted matches.
pub(crate) const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();

/// One input of a scripted player: what player `slot` orders, sent for `stamp`, as a plan its
/// match resolves to the units that stand when it is sent.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scripted<P> {
    stamp: u64,
    slot: u32,
    plan: P,
}

/// Where a scripted cast aims.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Aim {
    Nothing,
    Point {
        x: i64,
        z: i64,
    },
    /// Player `slot`'s hero.
    Hero {
        slot: u32,
    },
    /// The point player `slot`'s hero stands at as the input is sent.
    HeroPoint {
        slot: u32,
    },
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

impl Aim {
    /// The target this aim names, with the units' stable ids in `units`.
    pub(crate) fn target(self, units: MatchUnits<'_>) -> ActionTarget {
        match self {
            Aim::Nothing => ActionTarget::None,
            Aim::Hero { slot } => ActionTarget::Unit(units.hero(slot)),
            Aim::HeroPoint { slot } => ActionTarget::Point(units.position(units.hero(slot))),
            Aim::Point { x, z } => {
                let at = Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
                ActionTarget::Point(Position::new(at).expect("a point of the map"))
            }
        }
    }
}
