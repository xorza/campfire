use campfire_common::Ticks;
use campfire_math::Num;
use campfire_script::{Frame, UnknownName};
use campfire_sim::MatchRate;
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
    pub fn of(number: &Number, names: EffectNames<'_>) -> Result<Self, UnknownName> {
        Ok(match number {
            Number::Value(value) => Self::Value(
                value
                    .to_num()
                    .expect("an effect list's book holds each number a sim number"),
            ),
            Number::Param(reference) => Self::Param(names.param(&reference.param)?),
        })
    }

    /// Its value in `frame`, a call of its action: 0 for a scaling param below zero, which its
    /// source's stats can make it, as the load checks every other number not negative.
    pub fn number(self, frame: &Frame) -> Num {
        match self {
            Self::Value(value) => value,
            Self::Param(at) => frame
                .param_at(at)
                .expect("an effect list's book holds each param it reads")
                .to_num()
                .expect("an effect list's book holds each param it reads a number")
                .max(Num::ZERO),
        }
    }

    /// Its value in `frame` as a duration in milliseconds, in ticks of `rate`: whole, as the load
    /// checks every duration, so its floor is exact.
    pub fn duration(self, frame: &Frame, rate: MatchRate) -> Ticks {
        let ms = self.number(frame).floor();
        let ms = u64::try_from(ms).expect("a duration's book holds it not negative");
        rate.duration(ms)
            .expect("a duration's book holds it within a u32 of milliseconds")
    }
}
