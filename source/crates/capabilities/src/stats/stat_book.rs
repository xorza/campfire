use std::collections::BTreeMap;

use bevy_ecs::resource::Resource;
use campfire_math::{Num, U256};
use campfire_sim::{TickRate, Ticks};

use crate::combat::attack_stats::AttackStats;
use crate::stats::modifiers::Modifiers;
use crate::stats::stat::{EngineStat, Stat};
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::stats::stats_data::StatsData;
use crate::units::tag_set::TagSet;
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
    rate: TickRate,
    max_move_speed: Num,
}

/// `bits` as a number, at the end of the range of numbers when past it.
fn saturate(bits: i128) -> Num {
    let bits = bits.clamp(i128::from(i64::MIN), i128::from(i64::MAX));
    Num::from_bits(i64::try_from(bits).expect("clamped to the range"))
}

/// What a stat's value sums from its base and its modifiers, in bits: the base and each add, each
/// percent, and the largest cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatTotals {
    add: i128,
    pct: i128,
    cut: i128,
}

impl StatTotals {
    /// `(add) × (1 + pct) × (1 − cut)`, the cut from 0 to 1, each sum stopped at the range of a
    /// number, the product rounded once to nearest and stopped at that range too.
    fn value(self) -> Num {
        let one = 1_i128 << Num::FRAC_BITS;
        let base = saturate(self.add).to_bits();
        let gain = saturate(one + self.pct).to_bits();
        let kept = one - self.cut.clamp(0, one);
        let negative = (base < 0) != (gain < 0);
        let magnitude = u128::from(base.unsigned_abs()) * u128::from(gain.unsigned_abs());
        let product = U256::product(magnitude, kept.cast_unsigned())
            .round_shr(2 * Num::FRAC_BITS)
            .map_or(i128::MAX, |bits| i128::try_from(bits).unwrap_or(i128::MAX));
        saturate(if negative { -product } else { product })
    }
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
        types: impl IntoIterator<Item = (UnitType, &'a StatsData)>,
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
        for (unit_type, data) in types {
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

    /// The stats of a unit of `unit_type` at `level` carrying `modifiers` into `values`, with
    /// `totals` to sum in: each `(base + Σ add) × (1 + Σ pct) × (1 − max cut)`, the base the
    /// type's value at that level, 0 where it gives none, and each modifier's change times its
    /// stacks, of the modifiers whose tags `takes_effect` lets act; only the largest cut counts,
    /// from 0 to 1. The product rounds once, to nearest; a sum past the range of a number stops at
    /// its end; then the stat's limits clamp the value.
    pub(crate) fn compute(
        &self,
        unit_type: UnitType,
        level: u32,
        modifiers: Option<&Modifiers>,
        takes_effect: impl Fn(TagSet) -> bool,
        totals: &mut Vec<StatTotals>,
        values: &mut Vec<Num>,
    ) {
        let first = unit_type.index() * self.stats.len();
        let levels = i128::from(level - 1);
        totals.clear();
        totals.extend((0..self.stats.len()).map(|at| {
            let base = self.growth.get(first + at).copied().flatten();
            StatTotals {
                add: base.map_or(0, |growth| {
                    i128::from(growth.base.to_bits())
                        + i128::from(growth.per_level.to_bits()) * levels
                }),
                pct: 0,
                cut: 0,
            }
        }));
        let held = modifiers.into_iter().flat_map(Modifiers::iter);
        for instance in held.filter(|instance| takes_effect(instance.tags)) {
            for share in &instance.stats {
                let total = &mut totals[usize::from(share.stat)];
                let change = i128::from(share.value.to_bits()) * i128::from(instance.stacks);
                match share.op {
                    StatOp::Add => total.add += change,
                    StatOp::Pct => total.pct += change,
                    StatOp::Cut => total.cut = total.cut.max(change),
                }
            }
        }
        values.clear();
        values.extend(
            totals
                .iter()
                .zip(&self.rules)
                .map(|(total, rule)| rule.clamp(total.value())),
        );
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

    /// How far a unit with `values` walks a tick: its `move_speed`, from 0 to the manifest's
    /// `max_move_speed`, divided by the tick rate, rounded once; `None` when the mode declares no
    /// move speed.
    pub(crate) fn step(&self, values: &[Num]) -> Option<Num> {
        let speed = self
            .engine(values, EngineStat::MoveSpeed)?
            .clamp(Num::ZERO, self.max_move_speed);
        let hz = i64::from(self.rate.hz().get());
        Some(speed.checked_div_int(hz).expect("a speed a tick fits"))
    }

    /// The ticks from the start of one attack of a unit with `values` to the next at the
    /// earliest: the tick rate over its `attack_speed` attacks a second, exactly, rounded up, the
    /// attacks at least one bit, and the period longer than `windup`; `None` when the mode
    /// declares no attack speed.
    pub(crate) fn period(&self, values: &[Num], windup: Ticks) -> Option<Ticks> {
        let speed = self.engine(values, EngineStat::AttackSpeed)?;
        // Attacks a second in bits times 2²⁴.
        let attacks = (i128::from(speed.to_bits()) << Num::FRAC_BITS).max(1 << Num::FRAC_BITS);
        let hz = self.rate.hz().get();
        Some(AttackStats::period_at(hz, attacks.cast_unsigned(), windup))
    }
}
