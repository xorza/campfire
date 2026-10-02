use campfire_capabilities::{Action, ActionTarget, Scalar};
use campfire_common::Tick;
use campfire_sim::StableId;
use serde::Deserialize;

use crate::error::OrderScriptError;

/// A player's orders for their avatar, each at a sim tick, in tick order, and optionally the tick
/// the player leaves after: what a bot plays. It reads from TOML: an optional `end`, and one
/// `[[order]]` table each, with its `tick` and one action: `move = [x, z]` in meters,
/// `attack = <unit>`, or `cast = <slot>`, with `target = <unit>` when the ability takes one. A
/// unit is its stable id, which a mode that spawns in a fixed order gives each match alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderScript {
    orders: Vec<ScriptedOrder>,
    end: Option<Tick>,
}

/// One order of a script: the sim tick it is sent in, and what the avatar does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedOrder {
    pub tick: Tick,
    pub action: Action,
}

impl OrderScript {
    /// The script `text` holds; an error when it is not TOML of this shape, when an order names
    /// no action or two, or a target without a cast, when a coordinate is past what a sim number
    /// holds, when the ticks do not grow, or when the script ends before an order.
    pub fn parse(text: &str) -> Result<OrderScript, OrderScriptError> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            end: Option<u64>,
            #[serde(default)]
            order: Vec<Entry>,
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
        }
        let file: File = toml::from_str(text).map_err(OrderScriptError::Toml)?;
        let mut orders = Vec::with_capacity(file.order.len());
        for Entry {
            tick,
            to,
            attack,
            cast,
            target,
        } in file.order
        {
            let action = match (to, attack, cast, target) {
                (Some(to), None, None, None) => {
                    let [x, z] = to.map(Scalar::to_num);
                    let (Some(x), Some(z)) = (x, z) else {
                        return Err(OrderScriptError::Coordinate { tick });
                    };
                    Action::Move { x, z }
                }
                (None, Some(target), None, None) => Action::Attack { target },
                (None, None, Some(slot), target) => Action::Slot {
                    slot,
                    target: target.map_or(ActionTarget::None, ActionTarget::Unit),
                },
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
        let end = file.end.map(Tick::new);
        if let (Some(end), Some(last)) = (end, orders.last())
            && end < last.tick
        {
            return Err(OrderScriptError::EndsEarly { end: end.get() });
        }
        Ok(OrderScript { orders, end })
    }

    pub fn orders(&self) -> &[ScriptedOrder] {
        &self.orders
    }

    /// The last tick the player plays, when the script names one.
    pub const fn end(&self) -> Option<Tick> {
        self.end
    }
}

#[cfg(test)]
mod tests;
