use std::ops::Range;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::{AnyKeptRows, ColumnRows, KeptRows, RowFill, Unit, View, ViewColumn};
use campfire_units::Owner;

use crate::gatherer::Gatherer;
use crate::mode_supply_data::ModeSupplyData;
use crate::supply::{SupplyParts, SupplySums};
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::{SupplyCostBook, UnitSupply};

/// The parts of a unit production adds to its row of the script view.
pub(crate) type RowReads = (
    Option<&'static Owner>,
    SupplyParts,
    Option<&'static Gatherer>,
);

/// What production adds to the script view: what each unit counts for of its player's supply, a
/// row each, and the mode's supply rules and costs, by which scripts read each player's supply as
/// the view read the match.
#[derive(Debug, Default)]
pub struct ProductionColumn {
    rows: KeptRows<SupplyRows>,
    costs: SupplyCostBook,
    rules: Option<ModeSupplyData>,
}

/// A unit's row of the production column: its player, none for a unit no player owns, what it
/// counts for, and the amount it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SupplyRow {
    owner: Option<PlayerSlot>,
    counted: UnitSupply,
    load: u32,
}

/// The rows of one read of the production column, and each player's sums of them, by slot,
/// added as each row joins.
#[derive(Debug, Default, PartialEq, Eq)]
struct SupplyRows {
    rows: Vec<SupplyRow>,
    sums: Vec<SupplySums>,
}

impl SupplyRows {
    fn push(&mut self, row: SupplyRow) {
        if let Some(owner) = row.owner {
            let at = owner.index();
            if self.sums.len() <= at {
                self.sums.resize(at + 1, SupplySums::default());
            }
            self.sums[at].add(row.counted);
        }
        self.rows.push(row);
    }
}

impl ColumnRows for SupplyRows {
    fn clear(&mut self) {
        self.rows.clear();
        self.sums.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        self.rows.reserve(rows.len());
        for &row in &from.rows[rows] {
            self.push(row);
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
        // A row holds what its unit counts for by the costs, so other costs fill every row again.
        self.rules = world.resource::<SupplyBook>().0;
        let costs = world.resource::<SupplyCostBook>();
        if costs.same(&self.costs) {
            return false;
        }
        self.costs = costs.clone();
        true
    }
}

impl ProductionColumn {
    /// Adds the row of a unit owned by `owner`, which counts for `counted` and carries `load`.
    pub fn push(&mut self, owner: Option<PlayerSlot>, counted: UnitSupply, load: u32) {
        self.rows.now_mut().push(SupplyRow {
            owner,
            counted,
            load,
        });
    }

    /// `unit.load`: the amount `unit` carries, 0 with none, or in a view with no production.
    pub fn load(unit: &Unit) -> i64 {
        let row = unit.row_index();
        let load = unit
            .view()
            .column(|column: &Self| column.rows.now().rows[row].load);
        i64::from(load.unwrap_or(0))
    }

    pub(crate) const fn costs(&self) -> &SupplyCostBook {
        &self.costs
    }

    /// `slot`'s supply as `view` read the match; `None` in a mode with no supply.
    pub(crate) fn supply(view: &View, slot: PlayerSlot) -> Option<ReadSupply> {
        view.column(|column: &Self| {
            let rules = column.rules?;
            let sums = column.rows.now().sums.get(slot.index());
            let total = sums.copied().unwrap_or_default();
            Some(ReadSupply {
                used: total.used,
                cap: total.cap(u64::from(rules.max)),
            })
        })
        .flatten()
    }

    /// Fills a row of the script view with what the unit counts for of its player's supply, and
    /// what it carries.
    pub(super) fn fill_row(
        (owner, parts, gatherer): ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, Self>,
    ) {
        let counted = parts
            .unit()
            .map_or_default(|unit| fill.column.costs().unit(unit));
        let load = gatherer
            .and_then(|gatherer| gatherer.load())
            .map_or(0, |load| load.amount);
        fill.column
            .push(owner.map(|owner| owner.slot()), counted, load);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_sum_each_players_supply_as_they_join_and_as_they_are_kept() {
        // Player 0 owns rows of (2 used, 0 given) and (3, 10); player 2 one of (1, 0); a row no
        // player owns counts for no one. Player 0 sums to (5, 10), player 1 to none.
        let row = |owner: Option<u32>, used, given| SupplyRow {
            owner: owner.map(PlayerSlot::new),
            counted: UnitSupply { used, given },
            load: 0,
        };
        let mut read = SupplyRows::default();
        for joined in [
            row(Some(0), 2, 0),
            row(None, 7, 7),
            row(Some(2), 1, 0),
            row(Some(0), 3, 10),
        ] {
            read.push(joined);
        }
        let sums = |rows: &SupplyRows| {
            [0, 1, 2, 3].map(|slot| rows.sums.get(slot).map(|sum| (sum.used, sum.cap(u64::MAX))))
        };
        let summed = [Some((5, 10)), Some((0, 0)), Some((1, 0)), None];
        assert_eq!(sums(&read), summed);
        // Kept whole, the next read sums alike; kept from its third row on, player 0's (3, 10)
        // and player 2's (1, 0) alone.
        let mut next = SupplyRows::default();
        next.push_from(&read, 0..4);
        assert_eq!(next, read);
        next.clear();
        next.push_from(&read, 2..4);
        assert_eq!(
            sums(&next),
            [Some((3, 10)), Some((0, 0)), Some((1, 0)), None]
        );
        assert_eq!(next.len(), 2);
    }
}
