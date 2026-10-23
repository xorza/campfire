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

/// A decoder of one capability's directives, as `Directive::decoder` gives it.
pub(crate) type DirectiveDecoder = fn(&[u8]) -> Option<Directive>;

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
    /// The decoder of `capability`'s directives, as its reader decodes the body of a command to
    /// it; none for a capability whose commands hold no directive.
    pub(crate) fn decoder(capability: Capability) -> Option<DirectiveDecoder> {
        match capability {
            Capability::Orders => Some(|body| OrdersDirective::decode(body).map(Self::Orders)),
            Capability::Progression => {
                Some(|body| ProgressionDirective::decode(body).map(Self::Progression))
            }
            Capability::Production => {
                Some(|body| ProductionDirective::decode(body).map(Self::Production))
            }
            Capability::Items => Some(|body| ItemsDirective::decode(body).map(Self::Items)),
            Capability::Combat
            | Capability::Stats
            | Capability::Abilities
            | Capability::Projectiles
            | Capability::Areas
            | Capability::Navigation
            | Capability::Vision
            | Capability::Mode
            | Capability::Interaction => None,
        }
    }

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

    /// Its command, of its directive, as `body` holds it.
    const fn command<'b>(&self, body: &'b [u8]) -> Command<'b> {
        Command {
            to: Addressee::Capability(self.directive.capability()),
            body,
        }
    }

    /// Appends the payload of this order alone to `out`, with no select, as a client sends it to
    /// the units the server commands for its player (04-capabilities/control.md, A select goes with
    /// each order until its sender knows the server commands those units); its body is encoded in
    /// `body`, a buffer the caller reuses.
    pub fn write_commanded_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        body.clear();
        self.encode_into(body);
        Command::write(&[self.command(body)], out);
    }

    /// Writes into `payloads` the inputs of this order, each of `max_len` bytes at most: a select
    /// of its units with its directive, or, for more units than one payload holds, a select of as
    /// many as it holds, then selects that continue it, the last with the directive
    /// (04-capabilities/control.md, A select holds what its player controls). Whether they fit: not
    /// when one unit and the directive pass `max_len`, and then `payloads` holds none.
    pub fn write_payloads(&self, max_len: u32, payloads: &mut OrderPayloads) -> bool {
        let max_len = usize::try_from(max_len).expect("a payload's length fits usize");
        payloads.clear();
        self.encode_into(&mut payloads.directive);
        let mut select = self.select();
        loop {
            if payloads.push(select, Some(self), max_len) {
                return true;
            }
            // The most units one payload holds alone, one left at least for the last.
            let (mut fits, mut past) = (0, select.units.len());
            while fits + 1 < past {
                let mid = usize::midpoint(fits, past);
                let part = Select {
                    units: &select.units[..mid],
                    ..select
                };
                if payloads.push(part, None, max_len) {
                    payloads.pop();
                    fits = mid;
                } else {
                    past = mid;
                }
            }
            if fits == 0 {
                payloads.clear();
                return false;
            }
            let (part, rest) = select.units.split_at(fits);
            let pushed = payloads.push(
                Select {
                    units: part,
                    ..select
                },
                None,
                max_len,
            );
            debug_assert!(pushed, "the part that fits is pushed");
            select = Select {
                selecting: Selecting::More,
                units: rest,
            };
        }
    }
}

/// The inputs of one order, end to end in one buffer, as `Order::write_payloads` writes them, with
/// the buffers it encodes in, so a sender that keeps it allocates for no order.
#[derive(Debug, Default)]
pub struct OrderPayloads {
    bytes: Vec<u8>,
    ends: Vec<usize>,
    body: Vec<u8>,
    directive: Vec<u8>,
}

impl OrderPayloads {
    pub const fn iter(&self) -> OrderPayloadsIter<'_> {
        OrderPayloadsIter {
            payloads: self,
            next: 0,
        }
    }

    fn clear(&mut self) {
        self.bytes.clear();
        self.ends.clear();
        self.directive.clear();
    }

    /// Appends the payload of `select`, then of `order`'s directive when given; whether it holds
    /// `max_len` bytes at most, as none is appended when it does not.
    fn push(&mut self, select: Select<'_>, order: Option<&Order>, max_len: usize) -> bool {
        let start = self.bytes.len();
        self.body.clear();
        select.encode_into(&mut self.body);
        let to_players = Command {
            to: Addressee::Players,
            body: &self.body,
        };
        match order {
            Some(order) => Command::write(
                &[to_players, order.command(&self.directive)],
                &mut self.bytes,
            ),
            None => Command::write(&[to_players], &mut self.bytes),
        }
        if self.bytes.len() - start > max_len {
            self.bytes.truncate(start);
            return false;
        }
        self.ends.push(self.bytes.len());
        true
    }

    /// Takes the last payload away.
    fn pop(&mut self) {
        self.ends.pop();
        self.bytes.truncate(self.ends.last().copied().unwrap_or(0));
    }
}

impl<'a> IntoIterator for &'a OrderPayloads {
    type Item = &'a [u8];
    type IntoIter = OrderPayloadsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The payloads of an `OrderPayloads`, in order.
#[derive(Debug, Clone)]
pub struct OrderPayloadsIter<'a> {
    payloads: &'a OrderPayloads,
    next: usize,
}

impl<'a> Iterator for OrderPayloadsIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let end = *self.payloads.ends.get(self.next)?;
        let start = self
            .next
            .checked_sub(1)
            .map_or(0, |before| self.payloads.ends[before]);
        self.next += 1;
        Some(&self.payloads.bytes[start..end])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.payloads.ends.len() - self.next;
        (left, Some(left))
    }
}

impl ExactSizeIterator for OrderPayloadsIter<'_> {}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_players::{OrderUnits, Select, Selecting};
    use campfire_sim::{Addressee, Command, StableId};

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
                let directive = Directive::decoder(capability).and_then(|decode| decode(body));
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
