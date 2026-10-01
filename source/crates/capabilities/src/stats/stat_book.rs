use std::collections::BTreeMap;

use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::TickRate;

use crate::stats::stat::{EngineStat, Stat};
use crate::stats::stat_rule::StatRule;
use crate::stats::stat_totals::StatTotals;
use crate::stats::stats_data::StatsData;
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
    /// The stats' places in the order the refresh computes them: each after every stat a live
    /// change of it reads.
    order: Vec<u16>,
    /// Each stat's position in `order`, by its place.
    positions: Vec<u16>,
    rate: TickRate,
    max_move_speed: Num,
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
        let stats_len = u16::try_from(stats.len()).expect("stats fit u16");
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
            order: (0..stats_len).collect(),
            positions: (0..stats_len).collect(),
            rate,
            max_move_speed,
        })
    }

    pub(crate) const fn rate(&self) -> TickRate {
        self.rate
    }

    /// The book with the stats in `order`, a permutation of their places, as a stat graph of
    /// the mode gives it.
    #[must_use]
    pub(crate) fn with_order(self, order: Vec<u16>) -> StatBook {
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert!(
            sorted.iter().copied().eq(0..self.len()),
            "an order holds every stat once"
        );
        let mut positions = vec![0; order.len()];
        for (position, &at) in (0..).zip(&order) {
            positions[usize::from(at)] = position;
        }
        StatBook {
            order,
            positions,
            ..self
        }
    }

    /// How many stats the mode declares.
    pub(crate) fn len(&self) -> u16 {
        u16::try_from(self.stats.len()).expect("stats fit u16")
    }

    /// The stats' places in the order the refresh computes them.
    pub(crate) fn order(&self) -> &[u16] {
        &self.order
    }

    /// The position of the stat at `at` in the order.
    pub(crate) fn position(&self, at: u16) -> u16 {
        self.positions[usize::from(at)]
    }

    /// Appends to `totals` a unit of `unit_type` at `level`'s totals of each stat before its
    /// modifiers.
    pub(crate) fn totals(&self, unit_type: UnitType, level: u32, totals: &mut Vec<StatTotals>) {
        totals.extend(
            (0..self.len()).map(|at| StatTotals::base(self.base_bits(unit_type, at, level))),
        );
    }

    /// The value of the stat at `at` that `totals` sum, within its rule's limits.
    pub(crate) fn value(&self, at: u16, totals: StatTotals) -> Num {
        self.rules[usize::from(at)].clamp(totals.value())
    }

    /// The value of a unit of `unit_type` at `level` of the stat at `at`, before its modifiers,
    /// in bits: `base + per_level × (level − 1)`, 0 where the type gives none.
    pub(crate) fn base_bits(&self, unit_type: UnitType, at: u16, level: u32) -> i128 {
        let first = unit_type.index() * self.stats.len();
        let growth = self.growth.get(first + usize::from(at)).copied().flatten();
        growth.map_or(0, |growth| {
            i128::from(growth.base.to_bits())
                + i128::from(growth.per_level.to_bits()) * i128::from(level.saturating_sub(1))
        })
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
}
