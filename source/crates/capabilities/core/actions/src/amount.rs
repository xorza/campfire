use campfire_core::{Frame, UnknownName};
use campfire_math::Num;
use campfire_stats::StatsCall;
use campfire_values::Number;

use crate::effect_names::EffectNames;

/// A number of a listed effect: a value, or the param at its place among its action's, which the
/// frame holds at the call's rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Amount {
    Value(Num),
    Param(usize),
}

impl Amount {
    /// `number`, which its effect list's book holds a sim number, its param resolved by
    /// `names`; an error for a param its action does not declare.
    pub fn of(number: &Number, names: &impl EffectNames) -> Result<Amount, UnknownName> {
        Ok(match number {
            Number::Value(value) => Amount::Value(
                value
                    .to_num()
                    .expect("an effect list's book holds each number a sim number"),
            ),
            Number::Param(reference) => Amount::Param(names.param(&reference.param)?),
        })
    }

    /// Its value in `frame`, a call of its action: 0 for a scaling param below zero, which its
    /// source's stats can make it, as the load checks every other number not negative.
    pub fn number(self, frame: &Frame) -> Num {
        match self {
            Amount::Value(value) => value,
            Amount::Param(at) => StatsCall::ability_value(frame, at)
                .to_num()
                .expect("an effect list's book holds each param it reads a number")
                .max(Num::ZERO),
        }
    }
}
