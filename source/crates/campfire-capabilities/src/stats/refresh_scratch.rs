use bevy_ecs::entity::Entity;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::stats::live_param::LiveParam;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_id::StatId;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_totals::StatTotals;
use crate::units::tag_set::TagSet;
use crate::units::unit_type::UnitType;

/// What a stats refresh holds while it runs, kept between runs so a refresh allocates nothing once
/// its buffers have grown: the units it refreshes, by stable id, each one's run of totals and of
/// values, as many as the stats, and the live changes they carry.
#[derive(Debug, Default)]
pub(crate) struct RefreshScratch {
    pub(crate) units: Vec<Refreshing>,
    /// The units' places, sorted by their stable ids.
    by_id: Vec<u32>,
    totals: Vec<StatTotals>,
    values: Vec<Num>,
    lives: Vec<LiveTerm>,
}

/// A unit a refresh computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refreshing {
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
    pub(crate) unit_type: UnitType,
    pub(crate) level: u32,
}

/// A live change of the stat at `stat` of the refreshing unit at `unit`, by `op`, times
/// `stacks`: `live` of `source` at `rank`, or `value`, the value it last had, when it does not
/// resolve or its source is gone; then the value this refresh computes, which goes back to the
/// share at `share` of the unit's modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiveTerm {
    pub(crate) unit: u32,
    stat: StatId,
    op: StatOp,
    stacks: u32,
    live: LiveParam,
    rank: u8,
    source: Option<StableId>,
    pub(crate) share: u32,
    pub(crate) value: Num,
}

impl RefreshScratch {
    /// Empties it for a new refresh.
    pub(crate) fn clear(&mut self) {
        self.units.clear();
        self.by_id.clear();
        self.totals.clear();
        self.values.clear();
        self.lives.clear();
    }

    /// Adds `unit`, carrying `modifiers`, of which those with a stack whose tags in
    /// `modifier_book` `takes_effect` lets act change its stats, as their changes there say: its
    /// totals before its live changes, and its live changes. Whether it carries a live change.
    pub(crate) fn add(
        &mut self,
        book: &StatBook,
        modifier_book: &ModifierBook,
        unit: Refreshing,
        modifiers: Option<&Modifiers>,
        takes_effect: impl Fn(TagSet) -> bool,
    ) -> bool {
        let at = u32::try_from(self.units.len()).expect("units fit u32");
        self.units.push(unit);
        let row = self.totals.len();
        book.totals(unit.unit_type, unit.level, &mut self.totals);
        let mut live = false;
        let held = modifiers.into_iter().flat_map(Modifiers::iter);
        let mut first_share = 0;
        for instance in held {
            let shares = first_share..first_share + instance.shares.len();
            first_share = shares.end;
            let entry = modifier_book.get(instance.id);
            if instance.stacks == 0 || !takes_effect(entry.tags) {
                continue;
            }
            for ((share, spec), share_at) in
                instance.shares.iter().zip(&entry.spec.stats).zip(shares)
            {
                let Some(param) = share.live else {
                    let change = i128::from(share.value.to_bits()) * i128::from(instance.stacks);
                    self.totals[row + spec.stat.index()].change(spec.op, change);
                    continue;
                };
                live = true;
                self.lives.push(LiveTerm {
                    unit: at,
                    stat: spec.stat,
                    op: spec.op,
                    stacks: instance.stacks,
                    live: param,
                    rank: instance.rank,
                    source: instance.source,
                    share: u32::try_from(share_at).expect("a unit's shares fit u32"),
                    value: share.value,
                });
            }
        }
        live
    }

    /// The live changes, each with the value the last compute gave it.
    pub(crate) fn lives(&self) -> &[LiveTerm] {
        &self.lives
    }

    /// Computes every unit's values, each stat for every unit in `book`'s order: first its live
    /// changes, each live param of its source in `params`, a refreshing unit as computed so far or
    /// another as `other` gives it, the value it last had when it does not resolve or its source
    /// is gone; then the stat's value from its totals.
    pub(crate) fn compute<'q>(
        &mut self,
        book: &StatBook,
        params: &ParamBook,
        other: impl Fn(StableId) -> Option<ParamSource<'q>>,
    ) {
        let count = usize::from(book.len());
        self.by_id.extend((0..).take(self.units.len()));
        let units = &self.units;
        self.by_id.sort_unstable_by_key(|&at| units[at as usize].id);
        self.lives
            .sort_unstable_by_key(|term| book.position(term.stat));
        self.values.resize(self.units.len() * count, Num::ZERO);
        let mut next = 0;
        for &stat in book.order() {
            while let Some(&term) = self.lives.get(next).filter(|term| term.stat == stat) {
                let source = term.source.map(|id| match self.find(id) {
                    Some(at) => {
                        let unit = self.units[at];
                        let values = &self.values[at * count..(at + 1) * count];
                        Some(ParamSource::new(book, unit.unit_type, unit.level, values))
                    }
                    None => other(id),
                });
                let value = match source {
                    Some(None) => None,
                    Some(Some(source)) => params.live_value(term.live, term.rank, Some(&source)),
                    None => params.live_value(term.live, term.rank, None),
                }
                .unwrap_or(term.value);
                self.lives[next].value = value;
                next += 1;
                let change = i128::from(value.to_bits()) * i128::from(term.stacks);
                let at = term.unit as usize * count + stat.index();
                self.totals[at].change(term.op, change);
            }
            for unit in 0..self.units.len() {
                let at = unit * count + stat.index();
                self.values[at] = book.value(stat, self.totals[at]);
            }
        }
    }

    /// The values `compute` gave the unit at `unit`, in the order of the stats.
    pub(crate) fn values(&self, unit: usize, count: usize) -> &[Num] {
        &self.values[unit * count..(unit + 1) * count]
    }

    /// The place of `unit` among the refreshing units.
    fn find(&self, unit: StableId) -> Option<usize> {
        let units = &self.units;
        let found = self
            .by_id
            .binary_search_by_key(&unit, |&at| units[at as usize].id);
        found.ok().map(|at| self.by_id[at] as usize)
    }
}
