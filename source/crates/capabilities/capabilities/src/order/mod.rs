use campfire_codec::Binary;
use campfire_items::ItemsDirective;
use campfire_orders::OrdersDirective;
use campfire_players::{CapabilityDirective, OrderUnits, Select, Selecting};
use campfire_production::ProductionDirective;
use campfire_progression::ProgressionDirective;
use campfire_sim::{Addressee, Capability, Command, StableId};
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

    /// Appends its directive, the body of its command, to `body`.
    fn encode_into(&self, body: &mut Vec<u8>) {
        match self.directive {
            Directive::Orders(directive) => Binary::encode_into(&directive, body),
            Directive::Progression(directive) => Binary::encode_into(&directive, body),
            Directive::Production(directive) => Binary::encode_into(&directive, body),
            Directive::Items(directive) => Binary::encode_into(&directive, body),
        }
    }

    /// Its select, which commands its units.
    fn select(&self) -> Select<'_> {
        Select {
            selecting: Selecting::New,
            units: self.units.get(),
        }
    }

    /// Appends the payload of this order alone to `out`: a select of its units, then its
    /// directive, as a sender that reads no ack sends each order (commanded-units.md, A select
    /// goes with each order until its sender knows the server commands those units). Both bodies
    /// are encoded in `body`, a buffer the caller reuses, so a client that sends orders every tick
    /// allocates for none of them.
    pub fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        body.clear();
        self.select().encode_into(body);
        let split = body.len();
        self.encode_into(body);
        let (select, directive) = body.split_at(split);
        let commands = [
            Command {
                to: Addressee::Players,
                body: select,
            },
            Command {
                to: Addressee::Capability(self.directive.capability()),
                body: directive,
            },
        ];
        Command::write(&commands, out);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_items::ItemsDirective;
    use campfire_orders::OrdersDirective;
    use campfire_players::{CapabilityDirective, OrderUnits, Select, Selecting};
    use campfire_production::ProductionDirective;
    use campfire_progression::ProgressionDirective;
    use campfire_sim::{Addressee, Capability, Command, StableId};

    use crate::order::{Directive, Order};

    impl Order {
        /// The orders of `payload`, each to the units the selects before it in the payload
        /// command, as a test reads what a bot sent; none for a payload that is no list of
        /// selects and orders, or of an order before any select.
        pub fn of_payload(payload: &[u8]) -> Option<Vec<Self>> {
            let (mut orders, mut units, mut whole) = (Vec::new(), Vec::new(), true);
            let mut named: Vec<StableId> = Vec::new();
            let read = Command::read(payload, |command, _| {
                let body = command.body;
                let capability = match command.to {
                    Addressee::Players => {
                        named.clear();
                        match Select::decode_into(body, &mut named) {
                            Some(Selecting::New) => units.clone_from(&named),
                            Some(Selecting::More) => {
                                units.extend_from_slice(&named);
                                units.sort_unstable();
                                units.dedup();
                            }
                            None => whole = false,
                        }
                        return;
                    }
                    Addressee::Capability(capability) => capability,
                };
                let directive = match capability {
                    Capability::Orders => OrdersDirective::decode(body).map(Directive::Orders),
                    Capability::Progression => {
                        ProgressionDirective::decode(body).map(Directive::Progression)
                    }
                    Capability::Production => {
                        ProductionDirective::decode(body).map(Directive::Production)
                    }
                    Capability::Items => ItemsDirective::decode(body).map(Directive::Items),
                    _ => None,
                };
                match directive.zip(OrderUnits::new(&units)) {
                    Some((directive, units)) => orders.push(Self { units, directive }),
                    None => whole = false,
                }
            });
            (read && whole).then_some(orders)
        }

        /// The body of its command: its directive as its capability reads it.
        pub fn encode(&self) -> Vec<u8> {
            let mut body = Vec::new();
            self.encode_into(&mut body);
            body
        }

        /// A payload of `orders` alone, a select and a command each, as a test or a check sends
        /// them.
        pub fn payload(orders: &[Self]) -> Vec<u8> {
            let mut bodies: Vec<(Addressee, Vec<u8>)> = Vec::new();
            for order in orders {
                let mut select = Vec::new();
                order.select().encode_into(&mut select);
                bodies.push((Addressee::Players, select));
                let to = Addressee::Capability(order.directive.capability());
                bodies.push((to, order.encode()));
            }
            let commands: Vec<Command<'_>> = bodies
                .iter()
                .map(|(to, body)| Command { to: *to, body })
                .collect();
            Command::encode(&commands)
        }
    }
}

#[cfg(test)]
mod tests;
