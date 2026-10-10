use std::path::Path;

use campfire_capabilities::{Action, ActionTarget, InputValue, ModeInput, Scalar};
use campfire_common::{Tick, Toml};
use campfire_sim::StableId;
use campfire_store::InputFile;
use serde::Deserialize;

use crate::order_script::error::{OrderScriptError, OrderScriptReadError};

pub(crate) mod error;

/// A player's orders for their avatar and their mode inputs, each at a sim tick, in tick order,
/// and optionally the tick the player leaves after: what a bot plays. It reads from TOML: an
/// optional `end`; one `[[order]]` table each, with its `tick` and one action: `move = [x, z]` in
/// meters, `attack = <unit>`, `cast = <slot>`, with `target = <unit>` when the ability takes one,
/// or `learn = <slot>`; and one `[[input]]` table each, with its `tick`, the mode input's `name`
/// and its `value`, a string or a list of strings, as the mode declares the input. A unit is its
/// stable id, which a mode that spawns in a fixed order gives each match alike.
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
    pub name: String,
    pub value: ScriptedValue,
}

/// A scripted mode input's value, of the input's declared type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ScriptedValue {
    Text(String),
    List(Vec<String>),
}

/// One order of a script: the sim tick it is sent in, and what the avatar does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedOrder {
    pub tick: Tick,
    pub action: Action,
}

impl ScriptedInput {
    /// Appends the payload of this mode input alone to `out`.
    pub(crate) fn write_payload(&self, out: &mut Vec<u8>) {
        let value = match &self.value {
            ScriptedValue::Text(text) => InputValue::String(text),
            ScriptedValue::List(texts) => {
                InputValue::StringList(texts.iter().map(String::as_str).collect())
            }
        };
        let input = ModeInput {
            name: &self.name,
            value,
        };
        out.extend_from_slice(&ModeInput::payload(&[input]));
    }
}

impl OrderScript {
    /// The most bytes an order script's file may hold: a TOML line or two for each order, a few
    /// kilobytes for a match's script, so a mebibyte holds thousands of orders and refuses a file
    /// of another kind before it is read whole.
    pub const MAX_FILE_LEN: usize = 1 << 20;

    /// The script in the file at `path`; an error for a file that does not read, and for text
    /// `parse` refuses.
    pub fn read(path: &Path) -> Result<OrderScript, OrderScriptReadError> {
        let text = InputFile::read_text(path, OrderScript::MAX_FILE_LEN)
            .map_err(OrderScriptReadError::Read)?;
        OrderScript::parse(&text).map_err(|error| OrderScriptReadError::Script {
            path: path.to_owned(),
            error,
        })
    }

    /// The script `text` holds; an error when it is not TOML of this shape, when an order names
    /// no action or two, or a target without a cast, when a coordinate is past what a sim number
    /// holds, when the ticks of its orders or of its inputs do not grow, or when the script ends
    /// before an order or an input.
    pub fn parse(text: &str) -> Result<OrderScript, OrderScriptError> {
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
            name: String,
            value: ScriptedValue,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            tick: u64,
            #[serde(rename = "move")]
            to: Option<[Scalar; 2]>,
            attack: Option<StableId>,
            cast: Option<u8>,
            target: Option<StableId>,
            learn: Option<u8>,
        }
        let file: File = Toml::parse(text).map_err(OrderScriptError::Toml)?;
        let mut orders = Vec::with_capacity(file.order.len());
        for Entry {
            tick,
            to,
            attack,
            cast,
            target,
            learn,
        } in file.order
        {
            let action = match (to, attack, cast, target, learn) {
                (Some(to), None, None, None, None) => {
                    let [x, z] = to.map(Scalar::to_num);
                    let (Some(x), Some(z)) = (x, z) else {
                        return Err(OrderScriptError::Coordinate { tick });
                    };
                    Action::Move { x, z }
                }
                (None, Some(target), None, None, None) => Action::Attack { target },
                (None, None, Some(slot), target, None) => Action::Slot {
                    slot,
                    target: target.map_or(ActionTarget::None, ActionTarget::Unit),
                },
                (None, None, None, None, Some(slot)) => Action::Learn { slot },
                _ => return Err(OrderScriptError::Action { tick }),
            };
            let tick = Tick::new(tick);
            if orders
                .last()
                .is_some_and(|last: &ScriptedOrder| last.tick > tick)
            {
                return Err(OrderScriptError::Unordered { tick: tick.get() });
            }
            orders.push(ScriptedOrder { tick, action });
        }
        let mut inputs: Vec<ScriptedInput> = Vec::with_capacity(file.input.len());
        for InputEntry { tick, name, value } in file.input {
            let tick = Tick::new(tick);
            if inputs.last().is_some_and(|last| last.tick > tick) {
                return Err(OrderScriptError::Unordered { tick: tick.get() });
            }
            inputs.push(ScriptedInput { tick, name, value });
        }
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
        Ok(OrderScript {
            orders,
            inputs,
            end,
        })
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
