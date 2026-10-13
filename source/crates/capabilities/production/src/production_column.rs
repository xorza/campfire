use std::ops::Range;

use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_core::{Dead, KeptRows, Owner, RowFill, Unit, UnitType, View, ViewColumn};

use crate::gatherer::Gatherer;
use crate::mode_supply_data::ModeSupplyData;
use crate::site::Site;
use crate::supply::SupplySums;
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::{SupplyCostBook, UnitSupply};
use crate::train_queue::TrainQueue;

/// The parts of a unit production adds to its row of the script view.
pub type RowReads = (
    Option<&'static Owner>,
    Option<&'static UnitType>,
    Has<Dead>,
    Option<&'static TrainQueue>,
    Has<Site>,
    Option<&'static Gatherer>,
);

/// What production adds to the script view: what each unit counts for of its player's supply, a
/// row each, and the mode's supply rules and costs, by which scripts read each player's supply as
/// the view read the match.
#[derive(Debug, Default)]
pub struct ProductionColumn {
    rows: KeptRows<Vec<SupplyRow>>,
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

/// A player's supply as scripts read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadSupply {
    pub(crate) used: u64,
    pub(crate) cap: u64,
}

impl ViewColumn for ProductionColumn {
    fn begin(&mut self, world: &World) -> bool {
        self.rows.begin();
        // A row holds what its unit counts for by the costs, so other costs fill every row again.
        self.rules = world.resource::<SupplyBook>().0;
        let costs = world.resource::<SupplyCostBook>();
        if costs.same(&self.costs) {
            return false;
        }
        self.costs = costs.clone();
        true
    }

    fn keep(&mut self, rows: Range<usize>) {
        self.rows.keep(rows);
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }

    fn same_as_kept(&self) -> bool {
        self.rows.same_as_kept()
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
            .column(|column: &ProductionColumn| column.rows.now()[row].load);
        i64::from(load.unwrap_or(0))
    }

    pub(crate) const fn costs(&self) -> &SupplyCostBook {
        &self.costs
    }

    /// `slot`'s supply as `view` read the match; `None` in a mode with no supply.
    pub(crate) fn supply(view: &View, slot: PlayerSlot) -> Option<ReadSupply> {
        view.column(|column: &ProductionColumn| {
            let rules = column.rules?;
            let mut total = SupplySums::default();
            let rows = column.rows.now().iter();
            for row in rows.filter(|row| row.owner == Some(slot)) {
                total.add(row.counted);
            }
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
        (owner, unit_type, dead, queue, site, gatherer): ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, ProductionColumn>,
    ) {
        let counted = unit_type
            .map(|&unit_type| fill.column.costs().unit(unit_type, dead, !site, queue))
            .unwrap_or_default();
        let load = gatherer
            .and_then(|gatherer| gatherer.load())
            .map_or(0, |load| load.amount);
        fill.column
            .push(owner.map(|owner| owner.slot()), counted, load);
    }
}
