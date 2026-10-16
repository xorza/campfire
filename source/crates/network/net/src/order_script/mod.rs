use std::path::Path;

use campfire_actions::ActionTarget;
use campfire_capabilities::{Action, Order};
use campfire_codec::Toml;
use campfire_common::{PlayerSlot, Tick};
use campfire_math::Num;
use campfire_mode::{InputValue, ModeInput};
use campfire_orders::OrdersAction;
use campfire_players::OrderUnits;
use campfire_production::BuildTarget;
use campfire_progression::ProgressionAction;
use campfire_sim::StableId;
use campfire_store::InputFile;
use campfire_units::PlayerUnits;
use campfire_values::{DeclaredName, Names, TypeScope, UnitType};
use serde::Deserialize;

use crate::events::bot_units_missing::UnitsMissing;
use crate::order_script::error::{OrderScriptError, OrderScriptReadError};

pub(crate) mod error;

/// A player's orders for their units and their mode inputs, each at a sim tick, in tick order,
/// and optionally the tick the player leaves after: what a bot plays. It reads from TOML: an
/// optional `end`; one `[[order]]` table each, with its `tick`, optionally `units = "<type>"`,
/// the player's living units of that unit type as the order is due, its avatar without, and one
/// action: `move = [x, z]` in meters, `attack = <unit>`, `cast = <slot>`, with `target = <unit>`
/// when the action takes one, `build = { slot, x, z, angle }`, a building placed at `x`, `z` in
/// meters and `angle` degrees, or `learn = <slot>`; and one `[[input]]` table each, with its
/// `tick`, the mode input's `name` and its `value`, a string or a list of strings, as the mode
/// declares the input. A unit is its stable id, which a mode that spawns in a fixed order gives
/// each match alike.
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

/// One order of a script: the sim tick it is sent in, the units it commands, and what they do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedOrder {
    pub tick: Tick,
    pub units: ScriptedUnits,
    pub action: Action,
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
    pub(crate) fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
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
        input.write_payload(body, out);
    }
}

impl ScriptedOrder {
    /// The order it gives for the player of `slot`: its action for its units as `players` holds
    /// them now, a type's by its name in `types`, gathered in `scratch`; why it has none.
    pub(crate) fn order(
        &self,
        slot: PlayerSlot,
        players: &PlayerUnits<'_, '_>,
        types: &Names<UnitType>,
        scratch: &mut Vec<StableId>,
    ) -> Result<Order, UnitsMissing> {
        let units = match &self.units {
            ScriptedUnits::Avatar => {
                let avatar = players.avatar(slot).ok_or(UnitsMissing::NoAvatar)?;
                OrderUnits::one(avatar.id)
            }
            ScriptedUnits::Typed(name) => {
                let unit_type = types
                    .named((TypeScope::Mode, name.as_str()))
                    .ok_or(UnitsMissing::UnknownType)?;
                players.typed(slot, unit_type, scratch);
                OrderUnits::new(scratch).ok_or(UnitsMissing::NoUnit)?
            }
        };
        Ok(Order {
            units,
            action: self.action,
        })
    }
}

impl OrderScript {
    /// The most bytes an order script's file may hold: a TOML line or two for each order, a few
    /// kilobytes for a match's script, so a mebibyte holds thousands of orders and refuses a file
    /// of another kind before it is read whole.
    pub const MAX_FILE_LEN: usize = 1 << 20;

    /// The script in the file at `path`; an error for a file that does not read, and for text
    /// `parse` refuses.
    pub fn read(path: &Path) -> Result<Self, OrderScriptReadError> {
        let text =
            InputFile::read_text(path, Self::MAX_FILE_LEN).map_err(OrderScriptReadError::Read)?;
        Self::parse(&text).map_err(|error| OrderScriptReadError::Script {
            path: path.to_owned(),
            error,
        })
    }

    /// The script `text` holds; an error when it is not TOML of this shape, when an order names
    /// no action or two, or a target without a cast, when a coordinate is past what a sim number
    /// holds, when the ticks of its orders or of its inputs do not grow, or when the script ends
    /// before an order or an input.
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
            name: String,
            value: ScriptedValue,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            tick: u64,
            units: Option<DeclaredName>,
            #[serde(rename = "move")]
            to: Option<[Num; 2]>,
            attack: Option<StableId>,
            cast: Option<u8>,
            target: Option<StableId>,
            build: Option<BuildEntry>,
            learn: Option<u8>,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct BuildEntry {
            slot: u8,
            x: Num,
            z: Num,
            angle: Num,
        }
        let file: File = Toml::parse(text).map_err(OrderScriptError::Toml)?;
        let mut orders = Vec::with_capacity(file.order.len());
        for Entry {
            tick,
            units,
            to,
            attack,
            cast,
            target,
            build,
            learn,
        } in file.order
        {
            let named = [
                to.is_some(),
                attack.is_some(),
                cast.is_some(),
                build.is_some(),
                learn.is_some(),
            ];
            match named.into_iter().filter(|&named| named).count() {
                0 => return Err(OrderScriptError::NoAction { tick }),
                1 => {}
                _ => return Err(OrderScriptError::ManyActions { tick }),
            }
            if target.is_some() && cast.is_none() {
                return Err(OrderScriptError::StrayTarget { tick });
            }
            let action = if let Some([x, z]) = to {
                Action::Orders(OrdersAction::Move { x, z })
            } else if let Some(target) = attack {
                Action::Orders(OrdersAction::Attack { target })
            } else if let Some(slot) = cast {
                Action::Orders(OrdersAction::Slot {
                    slot,
                    target: target.map_or(ActionTarget::None, ActionTarget::Unit),
                })
            } else if let Some(BuildEntry { slot, x, z, angle }) = build {
                Action::Orders(OrdersAction::Build {
                    slot,
                    target: BuildTarget::Point { x, z, angle },
                })
            } else {
                let slot = learn.expect("an order names one action");
                Action::Progression(ProgressionAction::Learn { slot })
            };
            let units = units.map_or(ScriptedUnits::Avatar, ScriptedUnits::Typed);
            let tick = Tick::new(tick);
            if orders
                .last()
                .is_some_and(|last: &ScriptedOrder| last.tick > tick)
            {
                return Err(OrderScriptError::Unordered { tick: tick.get() });
            }
            orders.push(ScriptedOrder {
                tick,
                units,
                action,
            });
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
        Ok(Self {
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
