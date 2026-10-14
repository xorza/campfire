use campfire_codec::Binary;
use campfire_items::ItemsAction;
use campfire_orders::OrdersAction;
use campfire_players::{CapabilityAction, OrderUnits};
use campfire_production::ProductionAction;
use campfire_progression::ProgressionAction;
use campfire_sim::{Capability, Command, StableId};
use serde::{Deserialize, Serialize};

/// An order to the units it names: the body of a command to the capability its action belongs
/// to. Players, bots and AI issue the same orders, and an order to a group is one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub units: OrderUnits,
    pub action: Action,
}

/// What an order tells its units to do, by the capability that owns it, which decodes and
/// applies it: one variant for each capability with orders, as a command names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Orders(OrdersAction),
    Progression(ProgressionAction),
    Production(ProductionAction),
    Items(ItemsAction),
}

impl Action {
    /// The capability that owns it, to which its order is a command.
    pub const fn capability(self) -> Capability {
        match self {
            Action::Orders(_) => OrdersAction::CAPABILITY,
            Action::Progression(_) => ProgressionAction::CAPABILITY,
            Action::Production(_) => ProductionAction::CAPABILITY,
            Action::Items(_) => ItemsAction::CAPABILITY,
        }
    }
}

impl From<OrdersAction> for Action {
    fn from(action: OrdersAction) -> Action {
        Action::Orders(action)
    }
}

impl From<ProgressionAction> for Action {
    fn from(action: ProgressionAction) -> Action {
        Action::Progression(action)
    }
}

impl From<ProductionAction> for Action {
    fn from(action: ProductionAction) -> Action {
        Action::Production(action)
    }
}

impl From<ItemsAction> for Action {
    fn from(action: ItemsAction) -> Action {
        Action::Items(action)
    }
}

impl Order {
    /// The order of `action` to `id` alone.
    pub fn one(id: StableId, action: impl Into<Action>) -> Order {
        Order {
            units: OrderUnits::one(id),
            action: action.into(),
        }
    }

    /// The body of its command: its units, then its action as its capability reads it.
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();
        self.encode_into(&mut body);
        body
    }

    fn encode_into(&self, body: &mut Vec<u8>) {
        let units = &self.units;
        match self.action {
            Action::Orders(action) => Binary::encode_into(&(units, action), body),
            Action::Progression(action) => Binary::encode_into(&(units, action), body),
            Action::Production(action) => Binary::encode_into(&(units, action), body),
            Action::Items(action) => Binary::encode_into(&(units, action), body),
        }
    }

    /// A payload of `orders` alone, one command each, to its action's capability.
    pub fn payload(orders: &[Order]) -> Vec<u8> {
        let bodies: Vec<Vec<u8>> = orders.iter().map(Order::encode).collect();
        let commands: Vec<Command<'_>> = orders
            .iter()
            .zip(&bodies)
            .map(|(order, body)| Command {
                capability: order.action.capability(),
                body,
            })
            .collect();
        Command::encode(&commands)
    }

    /// Appends the payload of this order alone to `out`, its body encoded in `body`, a buffer the
    /// caller reuses, so a client that sends orders every tick allocates for none of them.
    pub fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        body.clear();
        self.encode_into(body);
        let command = Command {
            capability: self.action.capability(),
            body,
        };
        Command::write(&[command], out);
    }
}

#[cfg(test)]
mod tests;
