use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_capabilities::Order;
use campfire_common::ErrorReport;
use campfire_common::{AfterLeave, Seat, Tick};
use campfire_math::{Num, Vec3};
use campfire_protocol::{LeaveReason, SeatPlan, ServerInput};

use crate::fixed_session::fixed_match::FixedMatch;
use crate::match_units::MatchUnits;
use campfire_runner::SessionWorld;

/// The tick rate of the scripted matches.
pub(crate) const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();

/// One cue of a scripted player `seat` in tick `stamp`: an order, as a plan its match resolves to
/// the units that stand when it is sent, or the player's join or leave, which its server logs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scripted<P> {
    stamp: u64,
    slot: u32,
    cue: Cue<P>,
}

/// What a scripted player does in a tick.
#[derive(Debug, Clone, Copy)]
enum Cue<P> {
    /// It sends the order of this plan.
    Order(P),
    /// It joins its slot, open before.
    Join,
    /// It leaves its slot, which its server reserves for it.
    Leave,
}

/// Where a scripted cast aims.
#[derive(Debug, Clone, Copy)]
pub enum CastAim {
    Nothing,
    Point {
        x: i64,
        z: i64,
    },
    /// Player `slot`'s hero.
    Hero {
        slot: u32,
    },
    /// The point player `seat`'s hero stands at as the input is sent.
    HeroPoint {
        slot: u32,
    },
}

/// What a scripted input orders, by the role of the units it names.
pub trait Plan: Copy {
    /// The order this plan of player `seat` stands for, with its units' stable ids in `units`.
    fn order(self, units: MatchUnits<'_>, seat: u32) -> Order;

    /// What the players of `slots` send at tick 0 of `fixed`, before their first order.
    fn start(fixed: &mut FixedMatch, slots: &[SeatPlan]);
}

impl<P: Plan> Scripted<P> {
    pub(crate) const fn new(stamp: u64, slot: u32, plan: P) -> Self {
        Self {
            stamp,
            slot,
            cue: Cue::Order(plan),
        }
    }

    /// Player `slot` joins its slot in tick `stamp`.
    pub(crate) const fn join(stamp: u64, slot: u32) -> Self {
        Self {
            stamp,
            slot,
            cue: Cue::Join,
        }
    }

    /// Player `slot` leaves its slot in tick `stamp`, which its server reserves for it.
    pub(crate) const fn leave(stamp: u64, slot: u32) -> Self {
        Self {
            stamp,
            slot,
            cue: Cue::Leave,
        }
    }

    /// Runs tick `tick` of `fixed`: first logs the joins and the leaves of `script` in it, then
    /// sends its orders stamped for it, each in a packet of its own, then runs it.
    pub(crate) fn play_tick(script: &[Self], fixed: &mut FixedMatch, tick: u64) {
        let now = script.iter().filter(|scripted| scripted.stamp == tick);
        let refused = |error| panic!("tick {tick}: {}", ErrorReport::of(&error));
        for scripted in now.clone() {
            match scripted.cue {
                Cue::Join => fixed.join(scripted.slot).unwrap_or_else(refused),
                Cue::Leave => {
                    let leave = ServerInput::Leave {
                        seat: Seat::new(scripted.slot),
                        reason: LeaveReason::Asked,
                        becomes: AfterLeave::Reserve,
                    };
                    fixed.serve(leave).unwrap_or_else(refused);
                }
                Cue::Order(_) => {}
            }
        }
        for scripted in now {
            if let Cue::Order(plan) = scripted.cue {
                let order = plan.order(MatchUnits::of(fixed.game()), scripted.slot);
                fixed.send(scripted.slot, Tick::new(tick), &Order::payload(&[order]));
            }
        }
        fixed.game_mut().run_tick_alone();
    }
}

impl CastAim {
    /// The target this aim names, with the units' stable ids in `units`.
    pub(crate) fn target(self, units: MatchUnits<'_>) -> ActionTarget {
        match self {
            Self::Nothing => ActionTarget::None,
            Self::Hero { slot } => ActionTarget::Unit(units.hero(slot)),
            Self::HeroPoint { slot } => ActionTarget::Point(units.position(units.hero(slot))),
            Self::Point { x, z } => {
                let at = Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
                ActionTarget::Point(at)
            }
        }
    }
}
