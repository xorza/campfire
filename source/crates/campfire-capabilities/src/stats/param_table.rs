use std::collections::BTreeMap;

use campfire_math::{Num, ProductSum};

use crate::stats::param_source::ParamSource;
use crate::stats::stat_id::StatId;
use crate::values::declared_name::DeclaredName;
use crate::values::name_table::NameTable;
use crate::values::param::Param;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;

/// The params of the abilities or the modifiers of a match, one run of names for each owner, as
/// a match reads them: a scaling param's stats as places among the stats, its ratios in one
/// buffer.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParamTable {
    params: NameTable<ParamValue>,
    ratios: Vec<StatRatio>,
}

/// A param: one value or one per rank, or a scaling table.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParamValue {
    Ranked(Ranked<Scalar>),
    Scaled(Scaled),
}

/// A scaling table: its base, its gain a level, and its ratios, from `ratios_start` to
/// `ratios_end` of the table's.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scaled {
    base: Ranked<Num>,
    per_level: Num,
    ratios_start: u32,
    ratios_end: u32,
}

/// What a scaling table reads of a stat of its source: the stat's value times `ratio`, or with
/// `bonus` the part of it above its type's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StatRatio {
    stat: StatId,
    ratio: Num,
    bonus: bool,
}

impl ParamTable {
    /// Adds a run of `params`, each stat a scaling table names at its place `stat` gives, and
    /// gives its index. Every value is a number, which the load checked.
    pub(crate) fn push(
        &mut self,
        params: &BTreeMap<DeclaredName, Param>,
        stat: impl Fn(&Stat) -> StatId,
    ) -> usize {
        let mut values = Vec::with_capacity(params.len());
        for (name, param) in params {
            let value = match param {
                Param::Ranked(ranked) => ParamValue::Ranked(ranked.clone()),
                Param::Scaling(scaling) => {
                    let first = self.ratios.len();
                    let whole = scaling.ratios.iter().map(|(name, &ratio)| StatRatio {
                        stat: stat(name),
                        ratio,
                        bonus: false,
                    });
                    let bonus = scaling.bonus.iter().map(|(name, &ratio)| StatRatio {
                        stat: stat(name),
                        ratio,
                        bonus: true,
                    });
                    self.ratios.extend(whole.chain(bonus));
                    ParamValue::Scaled(Scaled {
                        base: scaling.base.clone(),
                        per_level: scaling.per_level,
                        ratios_start: u32::try_from(first).expect("ratios fit u32"),
                        ratios_end: u32::try_from(self.ratios.len()).expect("ratios fit u32"),
                    })
                }
            };
            values.push((name.as_str(), value));
        }
        self.params.push(values)
    }

    /// The place of param `name` in run `run`.
    pub(crate) fn named(&self, run: usize, name: &str) -> Option<usize> {
        self.params.named(run, name)
    }

    /// Whether it holds run `run`.
    pub(crate) const fn has_run(&self, run: usize) -> bool {
        self.params.has_run(run)
    }

    /// How many params run `run` holds.
    pub(crate) fn len(&self, run: usize) -> usize {
        self.params.values(run).len()
    }

    /// Whether the param at `at` of run `run` is a scaling table, whose value follows its source.
    pub(crate) fn scales(&self, run: usize, at: usize) -> bool {
        matches!(self.params.values(run)[at], ParamValue::Scaled(_))
    }

    /// Whether every param of run `run` has a value at `rank`.
    pub(crate) fn holds_rank(&self, run: usize, rank: u8) -> bool {
        (0..self.len(run)).all(|at| self.has_rank(run, at, rank))
    }

    /// Whether the param at `at` of run `run` has a value at `rank`.
    pub(crate) fn has_rank(&self, run: usize, at: usize, rank: u8) -> bool {
        match &self.params.values(run)[at] {
            ParamValue::Ranked(ranked) => ranked.get(rank).is_some(),
            ParamValue::Scaled(scaled) => scaled.base.get(rank).is_some(),
        }
    }

    /// The param at `at` of run `run` at `rank`, a scaling table's of `source`: `base + per_level
    /// × (level − 1) + Σ ratio × stat + Σ bonus ratio × (stat − the type's value)`, summed
    /// exactly, rounded once, and at the end of the range of numbers when past it; with no
    /// source, level 1 and stats of 0. Each param holds `rank`, as the load or the call checked.
    pub(crate) fn value(
        &self,
        run: usize,
        at: usize,
        rank: u8,
        source: Option<&ParamSource<'_>>,
    ) -> Scalar {
        const HELD: &str = "a param holds the rank its way checked";
        let scaled = match &self.params.values(run)[at] {
            ParamValue::Ranked(ranked) => return ranked.at(rank).expect(HELD),
            ParamValue::Scaled(scaled) => scaled,
        };
        let one = 1_i128 << Num::FRAC_BITS;
        let levels = i128::from(source.map_or(1, ParamSource::level).saturating_sub(1));
        let mut sum = ProductSum::ZERO;
        sum.add(scaled.base.at(rank).expect(HELD).to_bits(), one);
        sum.add(scaled.per_level.to_bits(), levels * one);
        if let Some(source) = source {
            let ratios = &self.ratios[scaled.ratios_start as usize..scaled.ratios_end as usize];
            for ratio in ratios {
                let stat = if ratio.bonus {
                    source.bonus_bits(ratio.stat)
                } else {
                    source.stat_bits(ratio.stat)
                };
                sum.add(ratio.ratio.to_bits(), stat);
            }
        }
        Scalar::Decimal(sum.saturating_num())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::stat_book::StatBook;
    use crate::units::unit_type::UnitType;
    use crate::values::param::Scaling;

    #[test]
    fn a_scaling_param_rounds_its_sum_once_ties_to_even_and_stops_at_the_range() {
        // A ratio of ε on a stat of ±0.5, ±1.5 and 2.5 is ±ε/2, ±3ε/2 and 5ε/2: each a tie, to
        // the even neighbour, 0, ±2ε and 2ε, as every rounding of a number. Away from zero they
        // were ±ε, ±2ε and 3ε.
        let power = Stat::named("power").unwrap();
        let armor = Stat::named("armor").unwrap();
        let table = |base: i64, ratios: &[(Stat, Num)]| {
            let scaling = Scaling {
                base: Ranked::One(Num::int(base)),
                per_level: Num::ZERO,
                bonus: BTreeMap::new(),
                ratios: ratios.iter().cloned().collect(),
            };
            let name = DeclaredName::new("p").unwrap();
            let params = BTreeMap::from([(name, Param::Scaling(scaling))]);
            let mut table = ParamTable::default();
            let at = |stat: &Stat| StatId::new(usize::from(*stat == armor));
            assert_eq!(table.push(&params, at), 0);
            table
        };
        let book = StatBook::new(&BTreeMap::new(), [], Num::ONE);
        let read = |table: &ParamTable, stats: [Num; 2]| {
            let source = ParamSource::new(&book, UnitType::new(0), 1, &stats);
            match table.value(0, 0, 1, Some(&source)) {
                Scalar::Decimal(value) => value,
                other @ Scalar::Int(_) => panic!("{other:?}"),
            }
        };
        let tiny = table(0, &[(power.clone(), Num::EPSILON)]);
        let bits = |stat: &str| read(&tiny, [stat.parse().unwrap(), Num::ZERO]).to_bits();
        let stats = ["0.5", "1.5", "2.5", "-0.5", "-1.5"];
        assert_eq!(stats.map(bits), [0, 2, 2, 0, -2]);
        // 3 + 2 × power − 2 × armor: at power and armor of `MAX` the two products, each past the
        // range, cancel, and leave 3; with one of them 0, the other alone passes the range, and
        // the sum stops at its end, `MAX` or `MIN`.
        let two = Num::int(2);
        let wide = table(3, &[(power, two), (armor.clone(), -two)]);
        assert_eq!(read(&wide, [Num::MAX, Num::MAX]), Num::int(3));
        assert_eq!(read(&wide, [Num::MAX, Num::ZERO]), Num::MAX);
        assert_eq!(read(&wide, [Num::ZERO, Num::MAX]), Num::MIN);
    }
}
