//! The server's bots: the order scripts, TOML of orders and mode inputs at ticks, their names
//! resolved in the match's books and map as each bot starts and each due order's slot and target
//! against the units it commands; the mode's thinking bots, which a Rhai script plays at its pace;
//! and the one driver of a tick's bots, which the server and every check play by.

mod bot_script;
mod bot_view;
mod loaded;
mod order_script;
mod thinking_bot;
mod tick_bots;

pub use crate::bot_script::bot_names::BotNames;
pub use crate::bot_script::error::{BotScriptError, OrderUnsent, ThinkError};
pub use crate::bot_script::{BotBuffers, BotOrder, BotScript, BotSink, BotUnits, OrderScratch};
pub use crate::bot_view::{BotView, ClientBotView, HeldBotView, ServerBotView};
pub use crate::order_script::error::OrderScriptError;
pub use crate::order_script::{
    OrderScript, ScriptedInput, ScriptedOrder, ScriptedUnits, ScriptedValue,
};
pub use crate::thinking_bot::{PacedBot, Think, ThinkingBot};
pub use crate::tick_bots::{PlayerSlotPlay, TickBot, TickBots, TickThinks};
