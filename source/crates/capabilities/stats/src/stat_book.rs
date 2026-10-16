use std::collections::BTreeMap;
use std::num::NonZeroI64;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_math::{Num, Rounding};
use campfire_script::{NameKind, UnknownName, View};
use campfire_sim::{Capability, TickRate};
use campfire_values::{EngineStat, NameId, Names, Stat, UnitType};

use crate::stat_id::StatId;
use crate::stat_rule::StatRule;
use crate::stat_totals::StatTotals;
use crate::stats_data::StatsData;

/// The stats of a match: each the mode declares, in order, with its rule, and each unit type's
/// value at level 1 and gain a level; with the move speed cap the engine's formulas take.
/// Package data, not state.
#[derive(Resource, Debug, Default)]
pub struct StatBook {
    /// The stats' names, by id; shared with the script view, which names them to scripts.
    names: Arc<Names<StatId>>,
    rules: Vec<StatRule>,
    /// Each engine stat's place among the stats, by the engine stat's order, when the mode
    /// declares it.
    engine: [Option<usize>; EngineStat::ALL.len()],
    /// Each unit type's growth of each stat, one row of the stats a type, in the types' order;
    /// `None` where the type gives the stat no value.
    growth: Vec<Option<Growth>>,
    /// The stats' places in the order the refresh computes them: each after every stat a live
    /// change of it reads.
    order: Vec<StatId>,
    /// Each stat's position in `order`, by its id.
    positions: Vec<u16>,
    max_move_speed: Num,
}

/// A stat's value at level 1, and what it gains a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Growth {
    base: Num,
    per_level: Num,
}

impl StatBook {
    /// The book of the stats `rules` declares, with no unit type's values yet, under the cap
    /// `max_move_speed`.
    pub fn new(rules: &BTreeMap<Stat, StatRule>, max_move_speed: Num) -> Self {
        let names: Names<StatId> = rules.keys().map(Stat::name).collect();
        let count = names.len();
        let stats_len = u16::try_from(count).expect("stats fit u16");
        let mut engine = [None; EngineStat::ALL.len()];
        for (at, stat) in rules.keys().enumerate() {
            if let Stat::Engine(stat) = stat {
                engine[*stat as usize] = Some(at);
            }
        }
        Self {
            order: (0..count).map(StatId::at).collect(),
            names: Arc::new(names),
            rules: rules.values().copied().collect(),
            engine,
            growth: Vec::new(),
            positions: (0..stats_len).collect(),
            max_move_speed,
        }
    }

    /// Gives `unit_type` the values of its `stats` section; an error, which gives it none, for a
    /// stat the book does not hold.
    pub fn add_type(&mut self, unit_type: UnitType, stats: &StatsData) -> Result<(), UnknownName> {
        if let Some(stat) = stats.0.keys().find(|stat| self.named(stat).is_none()) {
            return Err(UnknownName::new(NameKind::Stat, stat.name()));
        }
        let count = self.names.len();
        let first = unit_type.index() * count;
        if self.growth.len() < first + count {
            self.growth.resize(first + count, None);
        }
        for (stat, value) in &stats.0 {
            let at = self.named(stat).expect("each stat was found above");
            self.growth[first + at.index()] = Some(Growth {
                base: value.base,
                per_level: value.per_level,
            });
        }
        Ok(())
    }

    /// The book with the stats in `order`, a permutation of their places, as a stat graph of
    /// the mode gives it.
    #[must_use]
    pub fn with_order(self, order: impl IntoIterator<Item = StatId>) -> Self {
        let order: Vec<StatId> = order.into_iter().collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert!(
            sorted
                .iter()
                .copied()
                .eq((0..self.names.len()).map(StatId::at)),
            "an order holds every stat once"
        );
        let mut positions = vec![0; order.len()];
        for (position, &at) in (0..).zip(&order) {
            positions[at.index()] = position;
        }
        Self {
            order,
            positions,
            ..self
        }
    }

    /// How many stats the mode declares.
    pub fn len(&self) -> u16 {
        u16::try_from(self.names.len()).expect("stats fit u16")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The stats in the order the refresh computes them.
    pub fn order(&self) -> &[StatId] {
        &self.order
    }

    /// The position of `stat` in the order.
    pub fn position(&self, stat: StatId) -> u16 {
        self.positions[stat.index()]
    }

    /// Appends to `totals` a unit of `unit_type` at `level`'s totals of each stat before its
    /// modifiers.
    pub(crate) fn totals(&self, unit_type: UnitType, level: u32, totals: &mut Vec<StatTotals>) {
        totals.extend(
            (0..self.names.len())
                .map(|at| StatTotals::base(self.base_bits(unit_type, StatId::at(at), level))),
        );
    }

    /// The value of `stat` that `totals` sum, within its rule's limits.
    pub fn value(&self, stat: StatId, totals: StatTotals) -> Num {
        self.rules[stat.index()].clamp(totals.value())
    }

    /// The value of each stat, by id, of a unit of `unit_type` at `level` with no modifiers, as a
    /// refresh computes it.
    pub fn base_values(&self, unit_type: UnitType, level: u32) -> Vec<Num> {
        let ids = (0..self.names.len()).map(StatId::at);
        ids.map(|stat| {
            let totals = StatTotals::base(self.base_bits(unit_type, stat, level));
            self.value(stat, totals)
        })
        .collect()
    }

    /// Whether `unit_type` gives `stat` a value of its own.
    pub fn gives(&self, unit_type: UnitType, stat: StatId) -> bool {
        let first = unit_type.index() * self.names.len();
        self.growth
            .get(first + stat.index())
            .is_some_and(Option::is_some)
    }

    /// The value of a unit of `unit_type` at `level` of `stat`, before its modifiers, in bits:
    /// `base + per_level × (level − 1)`, 0 where the type gives none.
    pub(crate) fn base_bits(&self, unit_type: UnitType, stat: StatId, level: u32) -> i128 {
        let first = unit_type.index() * self.names.len();
        let growth = self.growth.get(first + stat.index()).copied().flatten();
        growth.map_or(0, |growth| {
            i128::from(growth.base.to_bits())
                + i128::from(growth.per_level.to_bits()) * i128::from(level.saturating_sub(1))
        })
    }

    /// The id of `stat`; `None` when the mode does not declare it.
    pub fn named(&self, stat: &Stat) -> Option<StatId> {
        self.names.named(stat.name())
    }

    /// Engine stat `stat` among `values`; `None` when the mode does not declare it.
    pub(crate) fn engine(&self, values: &[Num], stat: EngineStat) -> Option<Num> {
        self.engine[stat as usize].map(|at| values[at])
    }

    /// How far a unit with `values` walks a tick at `rate`: its `move_speed`, from 0 to the
    /// manifest's `max_move_speed`, divided by the tick rate, rounded once; `None` when the mode
    /// declares no move speed.
    pub fn step(&self, values: &[Num], rate: TickRate) -> Option<Num> {
        let speed = self
            .engine(values, EngineStat::MoveSpeed)?
            .clamp(Num::ZERO, self.max_move_speed);
        let hz = NonZeroI64::from(rate.hz()).get();
        Some(
            speed
                .checked_div_int(hz, Rounding::NearestEven)
                .expect("a speed a tick fits"),
        )
    }

    /// How far a unit with `values` turns a tick at `rate`, in degrees: its `turn_rate`, at
    /// least 0, divided by the tick rate, rounded once; `None` when the mode declares no turn
    /// rate.
    pub fn turn_step(&self, values: &[Num], rate: TickRate) -> Option<Num> {
        let turn = self.engine(values, EngineStat::TurnRate)?.max(Num::ZERO);
        let hz = NonZeroI64::from(rate.hz()).get();
        Some(
            turn.checked_div_int(hz, Rounding::NearestEven)
                .expect("a turn a tick fits"),
        )
    }
}

impl Book for StatBook {
    const OWNER: Option<Capability> = Some(Capability::Stats);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View) {
        view.names_mut().set_table(Arc::clone(&self.names));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;
    use campfire_values::{Stat, UnitType};

    use crate::stat_book::StatBook;
    use crate::stats_data::{StatValue, StatsData};

    impl StatBook {
        /// Gives `unit_type` `values` at level 1 by the stats' places, with no gain a level, as
        /// a `stats` section of each of the book's stats, in their order, would.
        pub(crate) fn add_base(&mut self, unit_type: UnitType, values: &[Num]) {
            debug_assert!(
                values.len() <= self.names.len(),
                "a value for a stat of the book"
            );
            let stats: StatsData = self
                .names
                .iter()
                .zip(values)
                .map(|((_, name), &base)| {
                    let stat = Stat::named(name).expect("a book's stat has a name");
                    let value = StatValue {
                        base,
                        per_level: Num::ZERO,
                    };
                    (stat, value)
                })
                .collect();
            self.add_type(unit_type, &stats)
                .expect("each stat is the book's");
        }
    }
}
