//! The bots' order scripts: the TOML a bot plays from, each order an `Action` that names its
//! slots, targets, places and item types; the names resolved in the match's books and map as its
//! bot starts, and each due order's slot and target against the units it commands.

mod bot_script;
mod bot_view;
mod loaded;
mod named;
mod order_script;
mod place;

pub use crate::bot_script::bot_names::BotNames;
pub use crate::bot_script::error::{BotScriptError, OrderUnsent};
pub use crate::bot_script::{BotBuffers, BotOrder, BotScript, BotSink, BotUnits, OrderScratch};
pub use crate::bot_view::{BotView, HeldBotView};
pub use crate::named::Named;
pub use crate::order_script::error::{OrderScriptError, OrderScriptReadError};
pub use crate::order_script::{
    OrderScript, ScriptedInput, ScriptedOrder, ScriptedUnits, ScriptedValue,
};
pub use crate::place::{MarkerPlace, Place};
