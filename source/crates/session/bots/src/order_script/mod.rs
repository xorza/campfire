use campfire_capabilities::{Directive, NamedRefs};
use campfire_codec::Toml;
use campfire_common::{DeclaredName, Tick};
use campfire_mode::{InputValue, ModeInput};
use serde::Deserialize;

use crate::order_script::error::OrderScriptError;

pub(crate) mod error;

/// A player's orders for their units and their mode inputs, each at a sim tick, in tick order,
/// and optionally the tick the player leaves after: what a bot plays. It reads from TOML: an
/// optional `end`; one `[[order]]` table each, with its `tick`, optionally `units = "<type>"`,
/// the player's living units of that unit type as the order is due, its avatar without, and its
/// `directive`, a `Directive` as serde reads it with names, such as
/// `directive.orders.slot = { slot = "<action>", target.unit = "enemies:avatar" }`; and one
/// `[[input]]` table each, with its `tick`, the mode input's `name` and its `value`, a string or
/// a list of strings, as the mode declares the input (04-capabilities/control.md, A bot's script
/// names what it orders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderScript {
    orders: Vec<ScriptedOrder>,
    inputs: Vec<ScriptedInput>,
    end: Option<Tick>,
}

/// One mode input of a script: the sim tick it is sent in, the input's name as the mode declares
/// it, and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedInput {
    pub tick: Tick,
    pub name: DeclaredName,
    pub value: ScriptedValue,
}

/// A scripted mode input's value, of the input's declared type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ScriptedValue {
    Text(String),
    List(Vec<String>),
}

/// One order of a script: the sim tick it is sent in, the units it commands, and what they do,
/// by names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedOrder {
    pub tick: Tick,
    pub units: ScriptedUnits,
    pub directive: Directive<NamedRefs>,
}

/// The units of its player an order commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptedUnits {
    /// The player's avatar.
    Avatar,
    /// The player's living units of the unit type of this name, as the order is due.
    Typed(DeclaredName),
}

impl ScriptedInput {
    /// Appends the payload of this mode input alone to `out`, its body written in `body`; a list
    /// of texts allocates the list of their slices.
    pub fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        let value = match &self.value {
            ScriptedValue::Text(text) => InputValue::String(text),
            ScriptedValue::List(texts) => {
                InputValue::StringList(texts.iter().map(String::as_str).collect())
            }
        };
        let input = ModeInput {
            name: self.name.as_str(),
            value,
        };
        input.write_payload(body, out);
    }
}

impl OrderScript {
    /// The most bytes an order script's file may hold: a TOML line or two for each order, a few
    /// kilobytes for a match's script, so a mebibyte holds thousands of orders and refuses a file
    /// of another kind before it is read whole.
    pub const MAX_FILE_LEN: usize = 1 << 20;

    /// The script `text` holds; an error when it is not TOML of this shape, when a coordinate is
    /// past what a sim number holds, when the ticks of its orders or of its inputs do not grow, or
    /// when the script ends before an order or an input.
    pub fn parse(text: &str) -> Result<Self, OrderScriptError> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            end: Option<u64>,
            #[serde(default)]
            order: Vec<Entry>,
            #[serde(default)]
            input: Vec<InputEntry>,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct InputEntry {
            tick: u64,
            name: DeclaredName,
            value: ScriptedValue,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            tick: u64,
            units: Option<DeclaredName>,
            directive: Directive<NamedRefs>,
        }
        let file: File = Toml::parse(text).map_err(OrderScriptError::Toml)?;
        let orders: Vec<ScriptedOrder> = file
            .order
            .into_iter()
            .map(
                |Entry {
                     tick,
                     units,
                     directive,
                 }| ScriptedOrder {
                    tick: Tick::new(tick),
                    units: units.map_or(ScriptedUnits::Avatar, ScriptedUnits::Typed),
                    directive,
                },
            )
            .collect();
        Self::growing(orders.iter().map(|order| order.tick))?;
        let inputs: Vec<ScriptedInput> = file
            .input
            .into_iter()
            .map(|InputEntry { tick, name, value }| ScriptedInput {
                tick: Tick::new(tick),
                name,
                value,
            })
            .collect();
        Self::growing(inputs.iter().map(|input| input.tick))?;
        let end = file.end.map(Tick::new);
        let last = orders
            .last()
            .map(|order| order.tick)
            .max(inputs.last().map(|input| input.tick));
        if let (Some(end), Some(last)) = (end, last)
            && end < last
        {
            return Err(OrderScriptError::EndsEarly { end: end.get() });
        }
        Ok(Self {
            orders,
            inputs,
            end,
        })
    }

    /// The script of `orders` alone, which never ends; an error when their ticks do not grow.
    pub fn of_orders(
        orders: impl IntoIterator<Item = ScriptedOrder>,
    ) -> Result<Self, OrderScriptError> {
        let orders: Vec<ScriptedOrder> = orders.into_iter().collect();
        Self::growing(orders.iter().map(|order| order.tick))?;
        Ok(Self {
            orders,
            inputs: Vec::new(),
            end: None,
        })
    }

    /// An error at the first of `ticks` below the one before it.
    fn growing(ticks: impl Iterator<Item = Tick>) -> Result<(), OrderScriptError> {
        let mut last = None;
        for tick in ticks {
            if last.is_some_and(|last| last > tick) {
                return Err(OrderScriptError::Unordered { tick: tick.get() });
            }
            last = Some(tick);
        }
        Ok(())
    }

    pub fn orders(&self) -> &[ScriptedOrder] {
        &self.orders
    }

    pub fn inputs(&self) -> &[ScriptedInput] {
        &self.inputs
    }

    /// The last tick the player plays, when the script names one.
    pub const fn end(&self) -> Option<Tick> {
        self.end
    }
}

#[cfg(test)]
mod tests;
