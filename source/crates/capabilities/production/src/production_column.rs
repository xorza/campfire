use std::ops::Range;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_actions::ByAction;
use campfire_common::Seat;
use campfire_math::Num;
use campfire_script::{
    AnyKeptRows, Checked, ColumnRows, IdNames, KeptRows, RowFill, ScriptText, ScriptValue, Unit,
    View, ViewColumn,
};
use campfire_units::Owner;
use campfire_values::{ActionId, Team, UnitType};

use crate::gatherer::Gatherer;
use crate::mode_supply_data::ModeSupplyData;
use crate::node::Node;
use crate::site::{Site, SiteProgress};
use crate::supply::{SupplyParts, SupplySums};
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::{SupplyCostBook, UnitSupply};
use crate::train_queue::TrainQueue;
use crate::train_spec::TrainSpec;

/// The parts of a unit production adds to its row of the script view.
pub(crate) type RowReads = (
    Option<&'static Owner>,
    SupplyParts,
    Option<&'static Gatherer>,
    Option<&'static Site>,
    Option<&'static SiteProgress>,
    Option<&'static TrainQueue>,
    Option<&'static Node>,
);

/// What production adds to the script view: what each unit counts for of its player's supply,
/// what it carries, builds, trains and holds, a row each; the mode's supply rules and costs, by
/// which scripts read each player's supply as the view read the match; and the trains' unit
/// types, by which they read a queue.
#[derive(Debug, Default)]
pub struct ProductionColumn {
    rows: KeptRows<ProductionRows>,
    costs: SupplyCostBook,
    rules: Option<ModeSupplyData>,
    trains: ByAction<TrainSpec>,
}

/// A unit's row of the production column: its player, none for a unit no player owns, what it
/// counts for, the amount it carries, the share of its site's time its progress reached, none
/// for no site, its queue's run of the column's trains, and the amount it holds as a node, none
/// for no node.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProductionRow {
    owner: Option<Seat>,
    counted: UnitSupply,
    load: u32,
    site: Option<Num>,
    queue: Range<u32>,
    node: Option<u32>,
}

/// The rows of one read of the production column, the queued trains of all, one run for each
/// row, and each player's sums of the rows, by slot, added as each row joins.
#[derive(Debug, Default, PartialEq, Eq)]
struct ProductionRows {
    rows: Vec<ProductionRow>,
    queued: Vec<ActionId>,
    sums: Vec<SupplySums>,
}

/// A unit's row as it joins, but its queue.
#[derive(Debug, Clone, Copy)]
struct RowOf {
    owner: Option<Seat>,
    counted: UnitSupply,
    load: u32,
    site: Option<Num>,
    node: Option<u32>,
}

impl ProductionRows {
    /// Adds `row`, whose unit queued `queue`, first to last.
    fn push(&mut self, row: RowOf, queue: impl IntoIterator<Item = ActionId>) {
        if let Some(owner) = row.owner {
            let at = owner.index();
            if self.sums.len() <= at {
                self.sums.resize(at + 1, SupplySums::default());
            }
            self.sums[at].add(row.counted);
        }
        let start = u32::try_from(self.queued.len()).expect("a view's trains fit a u32");
        self.queued.extend(queue);
        let end = u32::try_from(self.queued.len()).expect("a view's trains fit a u32");
        self.rows.push(ProductionRow {
            owner: row.owner,
            counted: row.counted,
            load: row.load,
            site: row.site,
            queue: start..end,
            node: row.node,
        });
    }

    fn queue(&self, row: usize) -> &[ActionId] {
        let Range { start, end } = self.rows[row].queue;
        &self.queued[start as usize..end as usize]
    }
}

impl ColumnRows for ProductionRows {
    fn clear(&mut self) {
        self.rows.clear();
        self.queued.clear();
        self.sums.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        self.rows.reserve(rows.len());
        for at in rows {
            let row = &from.rows[at];
            let joins = RowOf {
                owner: row.owner,
                counted: row.counted,
                load: row.load,
                site: row.site,
                node: row.node,
            };
            self.push(joins, from.queue(at).iter().copied());
        }
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

/// A player's supply as scripts read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadSupply {
    pub(crate) used: u64,
    pub(crate) cap: u64,
}

impl ViewColumn for ProductionColumn {
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, world: &World) -> bool {
        // A row holds what its unit counts for by the costs, so other costs fill every row again;
        // it holds its queue by train, which the column's trains name at each read.
        self.rules = world.resource::<SupplyBook>().0;
        self.trains = world.resource::<ByAction<TrainSpec>>().clone();
        let costs = world.resource::<SupplyCostBook>();
        if costs.same(&self.costs) {
            return false;
        }
        self.costs = costs.clone();
        true
    }

    /// None: it reads no team's sight.
    fn hides(&self, _: usize, _: Team) -> bool {
        false
    }
}

impl ProductionColumn {
    /// `unit.load`: the amount `unit` carries, 0 with none, or in a view with no production.
    pub fn load(unit: &Unit) -> Checked<i64> {
        let row = unit.row_index()?;
        let load = unit
            .view()
            .column(|column: &Self| column.rows.now().rows[row].load);
        Ok(i64::from(load.unwrap_or(0)))
    }

    /// `unit.site_progress`: the share of its site's time `unit`'s progress reached, from 0 to 1,
    /// `()` for no site, or in a view with no production.
    pub(crate) fn site_progress(unit: &Unit) -> Checked<ScriptValue> {
        let row = unit.row_index()?;
        let site = unit
            .view()
            .column(|column: &Self| column.rows.now().rows[row].site);
        Ok(site.flatten().map_or(ScriptValue::UNIT, ScriptValue::of))
    }

    /// `unit.queue`: the unit types of the trains `unit` queued, by name, first the one under way;
    /// none for no queue, or in a view with no production.
    pub(crate) fn queue(unit: &Unit) -> Checked<IdNames<UnitType>> {
        let row = unit.row_index()?;
        let view = unit.view();
        let queue = view.column(|column: &Self| {
            let names = column.rows.now().queue(row).iter().map(|&action| {
                let train = column.trains.get(action).expect("a queue holds trains");
                ScriptText::from(view.names().name(train.unit_type))
            });
            names.collect()
        });
        Ok(queue.unwrap_or_default())
    }

    /// `unit.node_amount`: the amount `unit` holds as a node, `()` for no node, or in a view with
    /// no production.
    pub(crate) fn node_amount(unit: &Unit) -> Checked<ScriptValue> {
        let row = unit.row_index()?;
        let node = unit
            .view()
            .column(|column: &Self| column.rows.now().rows[row].node);
        Ok(node.flatten().map_or(ScriptValue::UNIT, |amount| {
            ScriptValue::of(i64::from(amount))
        }))
    }

    pub(crate) const fn costs(&self) -> &SupplyCostBook {
        &self.costs
    }

    /// `seat`'s supply as `view` read the match; `None` in a mode with no supply.
    pub(crate) fn supply(view: &View, seat: Seat) -> Option<ReadSupply> {
        view.column(|column: &Self| {
            let rules = column.rules?;
            let sums = column.rows.now().sums.get(seat.index());
            let total = sums.copied().unwrap_or_default();
            Some(ReadSupply {
                used: total.used,
                cap: total.cap(u64::from(rules.max)),
            })
        })
        .flatten()
    }

    /// Fills a row of the script view with what the unit counts for of its player's supply, what
    /// it carries, its site's progress, its queue, and what it holds as a node.
    pub(super) fn fill_row(
        (owner, parts, gatherer, site, progress, queue, node): ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, Self>,
    ) {
        let counted = parts
            .unit()
            .map_or_default(|unit| fill.column.costs().unit(unit));
        let load = gatherer
            .and_then(|gatherer| gatherer.load())
            .map_or(0, |load| load.amount);
        let row = RowOf {
            owner: owner.map(|owner| owner.seat()),
            counted,
            load,
            site: site
                .zip(progress)
                .map(|(site, progress)| progress.share(site)),
            node: node.map(|node| node.amount()),
        };
        let queue = queue.map_or(&[][..], TrainQueue::entries);
        fill.column
            .rows
            .now_mut()
            .push(row, queue.iter().map(|queued| queued.action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_sum_each_players_supply_and_keep_their_queues_as_they_join_and_are_kept() {
        // Player 0 owns rows of (2 used, 0 given) and (3, 10); player 2 one of (1, 0); a row no
        // player owns counts for no one. Player 0 sums to (5, 10), player 1 to none. The rows
        // queue trains 0 and 1, none, 2, and 3, 4 and 5: runs 0..2, 2..2, 2..3 and 3..6.
        let row = |owner: Option<u32>, used, given| RowOf {
            owner: owner.map(Seat::new),
            counted: UnitSupply { used, given },
            load: 0,
            site: None,
            node: None,
        };
        let train = |ids: &[u32]| ids.iter().map(|&at| ActionId::new(at)).collect::<Vec<_>>();
        let mut read = ProductionRows::default();
        for (joined, queue) in [
            (row(Some(0), 2, 0), train(&[0, 1])),
            (row(None, 7, 7), train(&[])),
            (row(Some(2), 1, 0), train(&[2])),
            (row(Some(0), 3, 10), train(&[3, 4, 5])),
        ] {
            read.push(joined, queue);
        }
        let sums = |rows: &ProductionRows| {
            [0, 1, 2, 3].map(|slot| rows.sums.get(slot).map(|sum| (sum.used, sum.cap(u64::MAX))))
        };
        let summed = [Some((5, 10)), Some((0, 0)), Some((1, 0)), None];
        assert_eq!(sums(&read), summed);
        let runs: Vec<Range<u32>> = read.rows.iter().map(|row| row.queue.clone()).collect();
        assert_eq!(runs, [0..2, 2..2, 2..3, 3..6]);
        assert_eq!(read.queue(3), train(&[3, 4, 5]));
        // Kept whole, the next read sums alike; kept from its third row on, player 0's (3, 10)
        // and player 2's (1, 0) alone, their queues now runs 0..1 and 1..4.
        let mut next = ProductionRows::default();
        next.push_from(&read, 0..4);
        assert_eq!(next, read);
        next.clear();
        next.push_from(&read, 2..4);
        assert_eq!(
            sums(&next),
            [Some((3, 10)), Some((0, 0)), Some((1, 0)), None]
        );
        assert_eq!(next.len(), 2);
        assert_eq!(
            (next.rows[1].queue.clone(), next.queue(1)),
            (1..4, &*train(&[3, 4, 5]))
        );
        assert_eq!(next.queue(0), train(&[2]));
    }
}
