use campfire_codec::Binary;
use campfire_items::ItemsDirective;
use campfire_orders::OrdersDirective;
use campfire_players::{CapabilityDirective, OrderUnits};
use campfire_production::ProductionDirective;
use campfire_progression::ProgressionDirective;
use campfire_sim::{Capability, Command, StableId};
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

pub(crate) mod location;
pub(crate) mod named_refs;

/// An order to the units it names: the body of a command to the capability its directive belongs
/// to. Players, bots and AI issue the same orders, and an order to a group is one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub units: OrderUnits,
    pub directive: Directive,
}

/// What an order tells its units to do, by the capability that owns it, which decodes and
/// applies it: one variant for each capability with orders, as a command names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum Directive<R: OrderRefs = Ids> {
    Orders(OrdersDirective<R>),
    Progression(ProgressionDirective<R>),
    Production(ProductionDirective<R>),
    Items(ItemsDirective<R>),
}

impl Copy for Directive {}

impl<R: OrderRefs> Directive<R> {
    /// The same directive with each reference as `refs` gives it (04-capabilities/control.md, A
    /// directive is generic over its references).
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<Directive<T>, M::Error> {
        Ok(match self {
            Self::Orders(directive) => Directive::Orders(directive.map_refs(refs)?),
            Self::Progression(directive) => Directive::Progression(directive.map_refs(refs)?),
            Self::Production(directive) => Directive::Production(directive.map_refs(refs)?),
            Self::Items(directive) => Directive::Items(directive.map_refs(refs)?),
        })
    }
}

impl Directive {
    /// The capability that owns it, to which its order is a command.
    pub const fn capability(self) -> Capability {
        match self {
            Self::Orders(_) => OrdersDirective::CAPABILITY,
            Self::Progression(_) => ProgressionDirective::CAPABILITY,
            Self::Production(_) => ProductionDirective::CAPABILITY,
            Self::Items(_) => ItemsDirective::CAPABILITY,
        }
    }
}

impl From<OrdersDirective> for Directive {
    fn from(directive: OrdersDirective) -> Self {
        Self::Orders(directive)
    }
}

impl From<ProgressionDirective> for Directive {
    fn from(directive: ProgressionDirective) -> Self {
        Self::Progression(directive)
    }
}

impl From<ProductionDirective> for Directive {
    fn from(directive: ProductionDirective) -> Self {
        Self::Production(directive)
    }
}

impl From<ItemsDirective> for Directive {
    fn from(directive: ItemsDirective) -> Self {
        Self::Items(directive)
    }
}

impl Order {
    /// The order of `directive` to `id` alone.
    pub fn one(id: StableId, directive: impl Into<Directive>) -> Self {
        Self {
            units: OrderUnits::one(id),
            directive: directive.into(),
        }
    }

    fn encode_into(&self, body: &mut Vec<u8>) {
        let units = &self.units;
        match self.directive {
            Directive::Orders(directive) => Binary::encode_into(&(units, directive), body),
            Directive::Progression(directive) => Binary::encode_into(&(units, directive), body),
            Directive::Production(directive) => Binary::encode_into(&(units, directive), body),
            Directive::Items(directive) => Binary::encode_into(&(units, directive), body),
        }
    }

    /// Appends the payload of this order alone to `out`, its body encoded in `body`, a buffer the
    /// caller reuses, so a client that sends orders every tick allocates for none of them.
    pub fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        body.clear();
        self.encode_into(body);
        let command = Command {
            capability: self.directive.capability(),
            body,
        };
        Command::write(&[command], out);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_items::ItemsDirective;
    use campfire_orders::OrdersDirective;
    use campfire_players::{CapabilityDirective, OrderUnits};
    use campfire_production::ProductionDirective;
    use campfire_progression::ProgressionDirective;
    use campfire_sim::{Capability, Command};

    use crate::order::{Directive, Order};

    impl Order {
        /// The orders of `payload`, one a command, as a test reads what a bot sent; none for a
        /// payload that is no list of orders.
        pub fn of_payload(payload: &[u8]) -> Option<Vec<Self>> {
            let (mut orders, mut units, mut whole) = (Vec::new(), Vec::new(), true);
            let read = Command::read(payload, |command, _| {
                units.clear();
                let body = command.body;
                let directive = match command.capability {
                    Capability::Orders => {
                        OrdersDirective::decode_into(body, &mut units).map(Directive::Orders)
                    }
                    Capability::Progression => ProgressionDirective::decode_into(body, &mut units)
                        .map(Directive::Progression),
                    Capability::Production => ProductionDirective::decode_into(body, &mut units)
                        .map(Directive::Production),
                    Capability::Items => {
                        ItemsDirective::decode_into(body, &mut units).map(Directive::Items)
                    }
                    _ => None,
                };
                match directive.zip(OrderUnits::new(&units)) {
                    Some((directive, units)) => orders.push(Self { units, directive }),
                    None => whole = false,
                }
            });
            (read && whole).then_some(orders)
        }

        /// The body of its command: its units, then its directive as its capability reads it.
        pub fn encode(&self) -> Vec<u8> {
            let mut body = Vec::new();
            self.encode_into(&mut body);
            body
        }

        /// A payload of `orders` alone, one command each, to its directive's capability, as a test
        /// or a check sends them.
        pub fn payload(orders: &[Self]) -> Vec<u8> {
            let bodies: Vec<Vec<u8>> = orders.iter().map(Self::encode).collect();
            let commands: Vec<Command<'_>> = orders
                .iter()
                .zip(&bodies)
                .map(|(order, body)| Command {
                    capability: order.directive.capability(),
                    body,
                })
                .collect();
            Command::encode(&commands)
        }
    }
}

#[cfg(test)]
mod tests;
