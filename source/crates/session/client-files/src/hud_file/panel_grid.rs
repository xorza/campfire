use std::collections::BTreeMap;
use std::num::NonZeroU8;

use serde::{Deserialize, Serialize};

use crate::keys_file::Key;
use crate::panel_icon::PanelIcon;
use crate::panel_place::PanelCommand;

/// The command panel's grid: its columns and rows, the order its places fill it in, and the look
/// of each command a look places on it (02-engine-core.md, A look places its buttons;
/// 02-engine-core.md, The panel's commands are the engine's orders); a mode of no grid lays 5
/// columns of 3 rows, its places across each row, as StarCraft II's command card does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PanelGrid {
    pub columns: NonZeroU8,
    pub rows: NonZeroU8,
    pub order: PlaceOrder,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub commands: BTreeMap<PanelCommand, ButtonLook>,
}

/// The order a grid's places fill it in: across each row, the next place to the right, or down
/// each column, the next place below, as Zero Hour's `ControlBar.wnd` lays its 14.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceOrder {
    #[default]
    Rows,
    Columns,
}

/// A button's icon and the key that presses it, none of each for a button of its text and no
/// key (02-engine-core.md, A button's look is its action's).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ButtonLook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<PanelIcon>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<Key>,
}

/// A cell of a grid: its column and its row, from 0 at the left and the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCell {
    pub column: u8,
    pub row: u8,
}

impl PanelGrid {
    /// The cell of the place `place`, the first 1; none past the grid's last.
    pub const fn cell(&self, place: NonZeroU8) -> Option<GridCell> {
        let at = place.get() - 1;
        let (columns, rows) = (self.columns.get(), self.rows.get());
        let (major, minor) = match self.order {
            PlaceOrder::Rows => (columns, rows),
            PlaceOrder::Columns => (rows, columns),
        };
        let (line, along) = (at / major, at % major);
        if line >= minor {
            return None;
        }
        Some(match self.order {
            PlaceOrder::Rows => GridCell {
                column: along,
                row: line,
            },
            PlaceOrder::Columns => GridCell {
                column: line,
                row: along,
            },
        })
    }
}

impl Default for PanelGrid {
    fn default() -> Self {
        Self {
            columns: NonZeroU8::new(5).expect("5 columns"),
            rows: NonZeroU8::new(3).expect("3 rows"),
            order: PlaceOrder::Rows,
            commands: BTreeMap::new(),
        }
    }
}
