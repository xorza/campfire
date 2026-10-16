use bevy_ecs::resource::Resource;
use campfire_capabilities::{Action, Order};
use campfire_common::{PlayerSlot, Tick};
use campfire_players::OrderUnits;
use campfire_sim::StableId;
use campfire_units::PlayerUnits;
use campfire_values::{Names, TypeScope, UnitType};

use crate::bot_script::error::UnitsMissing;
use crate::order_script::error::UnknownUnitType;
use crate::order_script::{OrderScript, ScriptedInput, ScriptedOrder, ScriptedUnits};

pub(crate) mod error;

/// A script a bot plays for its player, each unit type it names resolved in the match's types:
/// each order goes out, for the player's units it names, and each mode input, in the tick of its
/// stamp, with the orders the player gives by hand.
#[derive(Resource, Debug, Clone)]
pub struct BotScript {
    orders: Vec<BotOrder>,
    inputs: Vec<ScriptedInput>,
    end: Option<Tick>,
    /// The first order not sent yet.
    next_order: usize,
    /// The first input not sent yet.
    next_input: usize,
}

/// One order of a bot's script: the sim tick it is sent in, the units it commands, and what they
/// do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotOrder {
    pub(crate) tick: Tick,
    pub units: BotUnits,
    pub action: Action,
}

/// The units of its player a bot's order commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotUnits {
    /// The player's avatar.
    Avatar,
    /// The player's living units of the type, as the order is due.
    Typed(UnitType),
}

impl BotScript {
    /// The bot that plays `script`, each unit type it names one of `types`, in the mode's scope;
    /// an error for the first it names that `types` lacks.
    pub fn new(script: &OrderScript, types: &Names<UnitType>) -> Result<Self, UnknownUnitType> {
        let orders = script.orders().iter().map(|order| {
            let units = match &order.units {
                ScriptedUnits::Avatar => BotUnits::Avatar,
                ScriptedUnits::Typed(name) => types
                    .named((TypeScope::Mode, name.as_str()))
                    .map(BotUnits::Typed)
                    .ok_or_else(|| UnknownUnitType { name: name.clone() })?,
            };
            Ok(BotOrder::of(order, units))
        });
        Ok(Self {
            orders: orders.collect::<Result<_, _>>()?,
            inputs: script.inputs().to_vec(),
            end: script.end(),
            next_order: 0,
            next_input: 0,
        })
    }

    /// The last tick the player plays, when the script names one.
    pub const fn end(&self) -> Option<Tick> {
        self.end
    }

    /// Takes the mode inputs due by `tick`, each once: a tick that passed while the match did not
    /// run sends its own late.
    pub fn due_inputs(&mut self, tick: Tick) -> &[ScriptedInput] {
        let first = self.next_input;
        self.next_input += self.inputs[first..].partition_point(|input| input.tick <= tick);
        &self.inputs[first..self.next_input]
    }

    /// Takes the orders due by `tick`, each once, as `due_inputs` takes the mode inputs.
    pub fn due_orders(&mut self, tick: Tick) -> &[BotOrder] {
        self.due_orders_while(tick, |_| true)
    }

    /// Takes the orders due by `tick` as `due_orders` does, but only up to the first that is not
    /// `ready`, which waits with those after it.
    pub fn due_orders_while(
        &mut self,
        tick: Tick,
        mut ready: impl FnMut(&BotOrder) -> bool,
    ) -> &[BotOrder] {
        let first = self.next_order;
        let due = self.orders[first..].partition_point(|order| order.tick <= tick);
        let taken = self.orders[first..first + due]
            .iter()
            .take_while(|order| ready(order))
            .count();
        self.next_order += taken;
        &self.orders[first..self.next_order]
    }
}

impl BotOrder {
    const fn of(order: &ScriptedOrder, units: BotUnits) -> Self {
        Self {
            tick: order.tick,
            units,
            action: order.action,
        }
    }

    /// The order it gives for the player of `slot`: its action for its units as `players` holds
    /// them now, gathered in `scratch`; why it has none.
    pub fn order(
        &self,
        slot: PlayerSlot,
        players: &PlayerUnits<'_, '_>,
        scratch: &mut Vec<StableId>,
    ) -> Result<Order, UnitsMissing> {
        let units = match self.units {
            BotUnits::Avatar => {
                let avatar = players.avatar(slot).ok_or(UnitsMissing::NoAvatar)?;
                OrderUnits::one(avatar.id)
            }
            BotUnits::Typed(unit_type) => {
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

#[cfg(test)]
mod tests;
