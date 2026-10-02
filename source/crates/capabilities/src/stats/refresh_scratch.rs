use bevy_ecs::entity::Entity;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::scripts::frame::Frame;
use crate::stats::live_param::LiveParam;
use crate::stats::modifiers::Modifiers;
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
/// `stacks`: `live` of `source` at `rank`, or `fallback` when it does not resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiveTerm {
    unit: u32,
    stat: StatId,
    op: StatOp,
    stacks: u32,
    live: LiveParam,
    rank: u8,
    source: Option<StableId>,
    fallback: Num,
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

    /// Adds `unit`, carrying `modifiers`, of which those whose tags `takes_effect` lets act
    /// change its stats: its totals before its live changes, and its live changes. Whether it
    /// carries a live change.
    pub(crate) fn add(
        &mut self,
        book: &StatBook,
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
        for instance in held.filter(|instance| takes_effect(instance.tags)) {
            for share in &instance.stats {
                let Some(param) = share.live else {
                    let change = i128::from(share.value.to_bits()) * i128::from(instance.stacks);
                    self.totals[row + share.stat.index()].change(share.op, change);
                    continue;
                };
                live = true;
                self.lives.push(LiveTerm {
                    unit: at,
                    stat: share.stat,
                    op: share.op,
                    stacks: instance.stacks,
                    live: param,
                    rank: instance.rank,
                    source: instance.source,
                    fallback: share.value,
                });
            }
        }
        live
    }

    /// Computes every unit's values, each stat for every unit in `book`'s order: first its live
    /// changes, each `frame`'s live param of its source, a refreshing unit as computed so far or
    /// another as `other` gives it, its fallback when it does not resolve; then the stat's value
    /// from its totals.
    pub(crate) fn compute<'q>(
        &mut self,
        book: &StatBook,
        frame: Option<&Frame>,
        other: impl Fn(StableId) -> Option<ParamSource<'q>>,
    ) {
        let count = usize::from(book.len());
        self.by_id.extend((0..).take(self.units.len()));
        let units = &self.units;
        self.by_id.sort_unstable_by_key(|&at| units[at as usize].id);
        self.lives.sort_by_key(|term| book.position(term.stat));
        self.values.resize(self.units.len() * count, Num::ZERO);
        let mut next = 0;
        for &stat in book.order() {
            while let Some(&term) = self.lives.get(next).filter(|term| term.stat == stat) {
                next += 1;
                let source = term.source.and_then(|id| {
                    let Some(at) = self.find(id) else {
                        return other(id);
                    };
                    let unit = self.units[at];
                    let values = &self.values[at * count..(at + 1) * count];
                    Some(ParamSource::new(book, unit.unit_type, unit.level, values))
                });
                let value = frame
                    .and_then(|frame| frame.live_value(term.live, term.rank, source.as_ref()))
                    .unwrap_or(term.fallback);
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
