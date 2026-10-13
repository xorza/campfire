use std::ops::Range;

use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;

use crate::production::gatherer::Gatherer;
use crate::production::mode_supply_data::ModeSupplyData;
use crate::production::site::Site;
use crate::production::supply::SupplySums;
use crate::production::supply_cost_book::{SupplyCostBook, UnitSupply};
use crate::production::train_queue::TrainQueue;
use crate::units::dead::Dead;
use crate::units::kept_rows::KeptRows;
use crate::units::owner::Owner;
use crate::units::row_fill::RowFill;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::units::view::View;
use crate::units::view_column::ViewColumn;

/// The parts of a unit production adds to its row of the script view.
pub(super) type RowReads = (
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
pub(crate) struct ProductionColumn {
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
    fn begin(&mut self, _: &World) -> bool {
        self.rows.begin();
        false
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
    /// Gives `view`'s column the mode's supply `rules`, none in a mode that counts no supply.
    pub(crate) fn share_rules(view: &View, rules: Option<ModeSupplyData>) {
        view.column_mut(|column: &mut ProductionColumn| column.rules = rules);
    }

    /// Gives `view`'s column what each unit type costs in supply.
    pub(crate) fn share_costs(view: &View, costs: SupplyCostBook) {
        view.column_mut(|column: &mut ProductionColumn| column.costs = costs);
    }

    /// Adds the row of a unit owned by `owner`, which counts for `counted` and carries `load`.
    pub(crate) fn push(&mut self, owner: Option<PlayerSlot>, counted: UnitSupply, load: u32) {
        self.rows.now_mut().push(SupplyRow {
            owner,
            counted,
            load,
        });
    }

    /// `unit.load`: the amount `unit` carries, 0 with none, or in a view with no production.
    pub(crate) fn load(unit: &Unit) -> i64 {
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
