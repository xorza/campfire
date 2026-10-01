use std::collections::BTreeMap;

use bevy_ecs::resource::Resource;
use campfire_math::{Num, U256};
use campfire_sim::{TickRate, Ticks};

use crate::combat::attack_stats::AttackStats;
use crate::stats::modifiers::Modifiers;
use crate::stats::stat::{EngineStat, Stat};
use crate::stats::stat_rule::{Combine, StatRule};
use crate::stats::stats_data::StatsData;
use crate::stats::unit_state::UnitState;
use crate::stats::unit_states::UnitStates;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;
use crate::values::scalar::Scalar;

/// The stats of a match: each the mode declares, in order, with its rule, and each unit type's
/// value at level 1 and gain a level; with the tick rate and the move speed cap the engine's
/// formulas take. Package data, not state.
#[derive(Resource, Debug)]
pub(crate) struct StatBook {
    stats: Vec<Stat>,
    rules: Vec<StatRule>,
    /// Each engine stat's place among the stats, by the engine stat's order, when the mode
    /// declares it.
    engine: [Option<usize>; EngineStat::ALL.len()],
    /// Each unit type's growth of each stat, one row of the stats a type, in the types' order;
    /// `None` where the type gives the stat no value.
    growth: Vec<Option<Growth>>,
    /// Each unit type's own states, as a tower's true sight.
    type_states: ByType<UnitStates>,
    rate: TickRate,
    max_move_speed: Num,
}

/// `bits` as a number, at the end of the range of numbers when past it.
fn saturate(bits: i128) -> Num {
    let bits = bits.clamp(i128::from(i64::MIN), i128::from(i64::MAX));
    Num::from_bits(i64::try_from(bits).expect("clamped to the range"))
}

/// A stat's value at level 1, and what it gains a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Growth {
    base: Num,
    per_level: Num,
}

impl StatBook {
    /// The book of the stats `rules` declares, for unit types `types`, each with its `stats`
    /// section, at `rate` under the cap `max_move_speed`; `None` when a type's value is not a
    /// number.
    pub(crate) fn new<'a>(
        rules: &BTreeMap<Stat, StatRule>,
        types: impl IntoIterator<Item = (UnitType, &'a StatsData, UnitStates)>,
        rate: TickRate,
        max_move_speed: Num,
    ) -> Option<StatBook> {
        let stats: Vec<Stat> = rules.keys().cloned().collect();
        let mut engine = [None; EngineStat::ALL.len()];
        for (at, stat) in stats.iter().enumerate() {
            if let Stat::Engine(stat) = stat {
                engine[*stat as usize] = Some(at);
            }
        }
        let mut growth = Vec::new();
        let mut type_states = ByType::default();
        for (unit_type, data, states) in types {
            type_states.set(unit_type, states);
            let first = unit_type.index() * stats.len();
            if growth.len() < first + stats.len() {
                growth.resize(first + stats.len(), None);
            }
            for (stat, value) in &data.0 {
                let Ok(at) = stats.binary_search(stat) else {
                    continue;
                };
                let per_level = value.per_level.map_or(Some(Num::ZERO), Scalar::to_num);
                growth[first + at] = Some(Growth {
                    base: value.base.to_num()?,
                    per_level: per_level?,
                });
            }
        }
        Some(StatBook {
            stats,
            rules: rules.values().copied().collect(),
            engine,
            growth,
            type_states,
            rate,
            max_move_speed,
        })
    }

    pub(crate) fn stats(&self) -> &[Stat] {
        &self.stats
    }

    pub(crate) const fn rate(&self) -> TickRate {
        self.rate
    }

    /// The stats of a unit of `unit_type` at `level` carrying `modifiers` into `values`: each
    /// the type's value at that level, 0 where it gives none, combined with each modifier's value
    /// times its stacks as the stat's rule says, then within the stat's limits. A value past the
    /// range of a number stops at its end before the limits.
    pub(crate) fn compute(
        &self,
        unit_type: UnitType,
        level: u32,
        modifiers: Option<&Modifiers>,
        values: &mut Vec<Num>,
    ) {
        let first = unit_type.index() * self.stats.len();
        let levels = i128::from(level - 1);
        values.extend((0..self.stats.len()).map(|at| {
            self.growth
                .get(first + at)
                .copied()
                .flatten()
                .map_or(Num::ZERO, |growth| {
                    saturate(
                        i128::from(growth.base.to_bits())
                            + i128::from(growth.per_level.to_bits()) * levels,
                    )
                })
        }));
        for instance in modifiers.into_iter().flat_map(Modifiers::iter) {
            for share in &instance.stats {
                let at = usize::from(share.stat);
                let total =
                    saturate(i128::from(share.value.to_bits()) * i128::from(instance.stacks));
                values[at] = match self.rules[at].combine {
                    Combine::Sum => {
                        saturate(i128::from(values[at].to_bits()) + i128::from(total.to_bits()))
                    }
                    Combine::Highest => values[at].max(total),
                };
            }
        }
        for (value, rule) in values.iter_mut().zip(&self.rules) {
            *value = rule.clamp(*value);
        }
    }

    /// The place of `stat` among the stats; `None` when the mode does not declare it.
    pub(crate) fn index(&self, stat: &Stat) -> Option<u16> {
        let at = self.stats.binary_search(stat).ok()?;
        Some(u16::try_from(at).expect("stats fit u16"))
    }

    /// Engine stat `stat` among `values`; `None` when the mode does not declare it.
    pub(crate) fn engine(&self, values: &[Num], stat: EngineStat) -> Option<Num> {
        self.engine[stat as usize].map(|at| values[at])
    }

    fn rule(&self, stat: EngineStat) -> Option<StatRule> {
        self.engine[stat as usize].map(|at| self.rules[at])
    }

    /// How far a unit with `values` walks a tick: `move_speed × (1 + move_speed_pct) × (1 −
    /// slow)`, within `move_speed`'s limits, at least 0 and at most the cap, divided by the tick
    /// rate, rounded once; `None` when the mode declares no move speed.
    /// The states of a unit of `unit_type` carrying `modifiers`: its type's and every
    /// instance's, whatever its stacks.
    pub(crate) fn states(&self, unit_type: UnitType, modifiers: Option<&Modifiers>) -> UnitStates {
        let own = self.type_states.get(unit_type).copied().unwrap_or_default();
        modifiers
            .into_iter()
            .flat_map(Modifiers::iter)
            .fold(own, |states, instance| states.union(instance.states))
    }

    pub(crate) fn step(&self, values: &[Num], states: UnitStates) -> Option<Num> {
        let speed = i128::from(self.engine(values, EngineStat::MoveSpeed)?.to_bits());
        let one = 1_i128 << Num::FRAC_BITS;
        let bits = |stat| {
            self.engine(values, stat)
                .map_or(0, |value| i128::from(value.to_bits()))
        };
        let gain = one + bits(EngineStat::MoveSpeedPct);
        let slow = if states.contains(UnitState::SlowImmune) {
            0
        } else {
            bits(EngineStat::Slow)
        };
        let kept = one - slow;
        let rule = self
            .rule(EngineStat::MoveSpeed)
            .expect("a declared move speed has a rule");
        let lower = rule.min.map_or(Num::ZERO, |min| min.max(Num::ZERO));
        let upper = rule
            .max
            .map_or(self.max_move_speed, |max| max.min(self.max_move_speed));
        let hz = i64::from(self.rate.hz().get());
        let per_tick = |speed: Num| speed.checked_div_int(hz).expect("a speed a tick fits");
        let first = speed * gain;
        if first <= 0 || kept <= 0 {
            return Some(per_tick(lower));
        }
        // In bits of a number times 2⁴⁸, the unrounded product of the three.
        let wide = U256::product(first.cast_unsigned(), kept.cast_unsigned());
        let scaled = |limit: Num| {
            U256::product(
                u128::try_from(limit.to_bits()).unwrap_or(0),
                1 << (2 * Num::FRAC_BITS),
            )
        };
        if wide >= scaled(upper) {
            return Some(per_tick(upper.max(lower)));
        }
        if wide <= scaled(lower) {
            return Some(per_tick(lower));
        }
        let divisor = u128::from(self.rate.hz().get()) << (2 * Num::FRAC_BITS);
        let step = wide.round_div(divisor).expect("a step below the cap fits");
        Some(Num::from_bits(
            i64::try_from(step).expect("a step below the cap fits"),
        ))
    }

    /// The ticks from the start of one attack of a unit with `values` to the next at the
    /// earliest: the tick rate over `attack_speed × (1 + attack_speed_pct)` attacks a second,
    /// exactly, rounded up, the attacks within `attack_speed`'s limits and at least one bit, and
    /// the period longer than `windup`; `None` when the mode declares no attack speed.
    pub(crate) fn period(&self, values: &[Num], windup: Ticks) -> Option<Ticks> {
        let speed = i128::from(self.engine(values, EngineStat::AttackSpeed)?.to_bits());
        let one = 1_i128 << Num::FRAC_BITS;
        let pct = self
            .engine(values, EngineStat::AttackSpeedPct)
            .map_or(0, |value| i128::from(value.to_bits()));
        // Attacks a second in bits times 2²⁴.
        let mut attacks = (speed * (one + pct)).max(one);
        if let Some(max) = self.rule(EngineStat::AttackSpeed).and_then(|rule| rule.max) {
            attacks = attacks.min((i128::from(max.to_bits()) << Num::FRAC_BITS).max(one));
        }
        let hz = self.rate.hz().get();
        Some(AttackStats::period_at(hz, attacks.cast_unsigned(), windup))
    }
}
