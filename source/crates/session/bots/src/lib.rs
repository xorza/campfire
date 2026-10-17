//! The bots' order scripts: the TOML a bot plays from, and each order's units resolved against
//! the match's unit types as its bot starts.

mod bot_script;
mod order_script;

pub use crate::bot_script::error::UnitsMissing;
pub use crate::bot_script::{BotBuffers, BotOrder, BotScript, BotSink, BotUnits};
pub use crate::order_script::error::{OrderScriptError, OrderScriptReadError, UnknownUnitType};
pub use crate::order_script::{
    OrderScript, ScriptedInput, ScriptedOrder, ScriptedUnits, ScriptedValue,
};
