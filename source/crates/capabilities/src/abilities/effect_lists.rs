use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::{StableId, TickRate};

use crate::abilities::effect_names::EffectNames;
use crate::actions::action_book::ActionId;
use crate::actions::action_data::ActionData;
use crate::actions::effect_data::EffectTo;
use crate::actions::effect_data::{EffectData, Effecting};
use crate::combat::combat_effect::CombatEffect;
use crate::combat::damage_kind::DamageKind;
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_id::TrackId;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::pool_id::PoolId;
use crate::values::number::Number;

/// The effect lists of each action, their names resolved as the action loaded: one buffer, and
/// by action id the run of each of its lists, `on_resolve`, `on_hit` and `on_end`. An action
/// past the end has none. Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct EffectLists {
    effects: Vec<Listed>,
    runs: Vec<[Range<u32>; 3]>,
}

/// An effect of a list: what it does, and to whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) does: Does,
    pub(crate) to: EffectTo,
}

/// What a listed effect does, its names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Does {
    Damage {
        amount: Amount,
        kind: DamageKind,
    },
    Heal {
        amount: Amount,
    },
    Restore {
        pool: PoolId,
        amount: Amount,
    },
    Modifier {
        id: ModifierId,
        duration_ms: Option<Amount>,
    },
    Xp {
        track: TrackId,
        amount: Amount,
    },
}

/// A number of a listed effect: a value, or the param at its place among its action's, which the
/// frame holds at the call's rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Amount {
    Value(Num),
    Param(usize),
}

impl EffectLists {
    /// Adds the lists of `action`, which loaded last: `on_resolve`, `on_hit` and `on_end`.
    pub(crate) fn push(&mut self, action: ActionId, lists: [Vec<Listed>; 3]) {
        if self.runs.len() <= action.index() {
            self.runs.resize(action.index() + 1, [0..0, 0..0, 0..0]);
        }
        self.runs[action.index()] = lists.map(|list| {
            let start = position(self.effects.len());
            self.effects.extend(list);
            start..position(self.effects.len())
        });
    }

    /// The list of `action` that runs before its `hook`: `on_resolve`, `on_hit` or `on_end`.
    pub(crate) fn of(&self, action: ActionId, hook: Hook) -> &[Listed] {
        let list = match hook {
            Hook::OnResolve => 0,
            Hook::OnHit => 1,
            Hook::OnEnd => 2,
            _ => unreachable!("only a resolve and a delivery's hit and end have lists"),
        };
        self.runs.get(action.index()).map_or(&[], |runs| {
            let run = &runs[list];
            &self.effects[run.start as usize..run.end as usize]
        })
    }

    /// Queues `list` in `frame`, a call of its action at its rank: each effect to `reached`, the
    /// unit the list reached, or to the acting unit; its durations at `rate`.
    pub(crate) fn queue(
        list: &[Listed],
        frame: &mut Frame,
        reached: Option<StableId>,
        rate: TickRate,
    ) {
        let acting = frame.acting();
        for listed in list {
            let unit = match listed.to {
                EffectTo::Reached => {
                    reached.expect("the load lets only an effect to the source reach no unit")
                }
                EffectTo::Source => acting.expect("an action's list runs for its acting unit"),
            };
            let number = |frame: &Frame, amount| Amount::number(amount, frame);
            match listed.does {
                Does::Damage { amount, kind } => {
                    let amount = number(frame, amount);
                    frame.effects.push(CombatEffect::Damage {
                        target: unit,
                        amount,
                        kind,
                    });
                }
                Does::Heal { amount } => {
                    let amount = number(frame, amount);
                    frame.effects.push(CombatEffect::Heal { unit, amount });
                }
                Does::Restore { pool, amount } => {
                    let amount = number(frame, amount);
                    frame
                        .effects
                        .push(CombatEffect::Restore { unit, pool, amount });
                }
                Does::Modifier { id, duration_ms } => {
                    let duration = duration_ms.map(|ms| {
                        // Whole, as the load checked, so the floor is exact.
                        let ms = number(frame, ms).floor();
                        let ms =
                            u64::try_from(ms).expect("the load checked a duration not negative");
                        rate.duration(ms)
                            .expect("the load checked a duration within reach")
                    });
                    frame.effects.push(ModifierEffect::Add {
                        target: unit,
                        id,
                        duration,
                    });
                }
                Does::Xp { track, amount } => {
                    let amount = number(frame, amount);
                    frame.effects.push(ProgressionEffect::AddXp {
                        unit,
                        track,
                        amount,
                    });
                }
            }
        }
    }
}

impl Listed {
    /// The lists of `data`, `on_resolve`, `on_hit` and `on_end`, which the package load checked,
    /// their names resolved by `names`.
    pub(crate) fn lists_of(data: &ActionData, names: &impl EffectNames) -> [Vec<Listed>; 3] {
        let amount = |number: &Number| match number {
            Number::Value(value) => Amount::Value(
                value
                    .to_num()
                    .expect("the load checked each number of an effect list"),
            ),
            Number::Param(reference) => Amount::Param(names.param(&reference.param)),
        };
        let resolve = |effect: &EffectData| {
            let does = match &effect.does {
                Effecting::Damage {
                    amount: number,
                    kind,
                } => Does::Damage {
                    amount: amount(number),
                    kind: names.damage_kind(kind),
                },
                Effecting::Heal { amount: number } => Does::Heal {
                    amount: amount(number),
                },
                Effecting::Restore {
                    pool,
                    amount: number,
                } => Does::Restore {
                    pool: names.pool(pool),
                    amount: amount(number),
                },
                Effecting::Modifier { id, duration_ms } => Does::Modifier {
                    id: names.modifier(id),
                    duration_ms: duration_ms.as_ref().map(amount),
                },
                Effecting::Xp {
                    track,
                    amount: number,
                } => Does::Xp {
                    track: names.track(track),
                    amount: amount(number),
                },
                Effecting::Planned(_) => unreachable!("the load refuses a planned effect"),
            };
            Listed {
                does,
                to: effect.to,
            }
        };
        [&data.on_resolve, &data.on_hit, &data.on_end]
            .map(|list| list.iter().map(resolve).collect::<Vec<_>>())
    }
}

impl Amount {
    /// Its value in `frame`, a call of its action: 0 for a scaling param below zero, which its
    /// source's stats can make it, as the load checks every other number not negative.
    fn number(self, frame: &Frame) -> Num {
        match self {
            Amount::Value(value) => value,
            Amount::Param(at) => frame
                .ability_value(at)
                .to_num()
                .expect("the load checked that an effect's param is a number")
                .max(Num::ZERO),
        }
    }
}

/// A position in the buffer, which a match's data keeps within `u32`.
fn position(len: usize) -> u32 {
    u32::try_from(len).expect("a match's effects fit u32")
}
