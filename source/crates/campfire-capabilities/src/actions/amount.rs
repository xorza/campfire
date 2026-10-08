use campfire_math::Num;

use crate::actions::effect_names::EffectNames;
use crate::scripts::frame::Frame;
use crate::stats::stats_call::StatsCall;
use crate::values::number::Number;

/// A number of a listed effect: a value, or the param at its place among its action's, which the
/// frame holds at the call's rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Amount {
    Value(Num),
    Param(usize),
}

impl Amount {
    /// `number`, its param resolved by `names`, which the load checked a sim number.
    pub(super) fn of(number: &Number, names: &impl EffectNames) -> Amount {
        match number {
            Number::Value(value) => Amount::Value(
                value
                    .to_num()
                    .expect("the load checked each number of an effect list"),
            ),
            Number::Param(reference) => Amount::Param(names.param(&reference.param)),
        }
    }

    /// Its value in `frame`, a call of its action: 0 for a scaling param below zero, which its
    /// source's stats can make it, as the load checks every other number not negative.
    pub(crate) fn number(self, frame: &Frame) -> Num {
        match self {
            Amount::Value(value) => value,
            Amount::Param(at) => StatsCall::ability_value(frame, at)
                .to_num()
                .expect("the load checked that an effect's param is a number")
                .max(Num::ZERO),
        }
    }
}
