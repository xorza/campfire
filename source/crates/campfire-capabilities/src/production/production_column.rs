use std::ops::Range;

use bevy_ecs::world::World;
use campfire_common::PlayerSlot;

use crate::production::supply_costs::{SupplyCosts, UnitSupply};
use crate::production::supply_rules::SupplyRules;
use crate::units::kept_rows::KeptRows;
use crate::units::script_view::View;
use crate::units::view_column::ViewColumn;

/// What production adds to the script view: what each unit counts for of its player's supply, a
/// row each, and the mode's supply rules and costs, by which scripts read each player's supply as
/// the view read the match.
#[derive(Debug, Default)]
pub(crate) struct ProductionColumn {
    rows: KeptRows<Vec<SupplyRow>>,
    costs: SupplyCosts,
    rules: Option<SupplyRules>,
}

/// A unit's row of the production column: its player, none for a unit no player owns, and what
/// it counts for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SupplyRow {
    owner: Option<PlayerSlot>,
    counted: UnitSupply,
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
    /// Gives `view`'s column the mode's supply `rules` and `costs`.
    pub(crate) fn share(view: &View, rules: Option<SupplyRules>, costs: SupplyCosts) {
        view.column_mut(|column: &mut ProductionColumn| {
            column.rules = rules;
            column.costs = costs;
        });
    }

    /// Adds the row of a unit owned by `owner`, which counts for `counted`.
    pub(crate) fn push(&mut self, owner: Option<PlayerSlot>, counted: UnitSupply) {
        self.rows.now_mut().push(SupplyRow { owner, counted });
    }

    pub(crate) const fn costs(&self) -> &SupplyCosts {
        &self.costs
    }

    /// `player`'s supply as `view` read the match; `None` in a mode with no supply.
    pub(crate) fn supply(view: &View, player: PlayerSlot) -> Option<ReadSupply> {
        view.column(|column: &ProductionColumn| {
            let rules = column.rules?;
            let rows = column
                .rows
                .now()
                .iter()
                .filter(|row| row.owner == Some(player));
            let (used, given) = rows.fold((0, 0), |(used, given), row| {
                (used + row.counted.used, given + row.counted.given)
            });
            Some(ReadSupply {
                used,
                cap: given.min(u64::from(rules.max)),
            })
        })
        .flatten()
    }
}
