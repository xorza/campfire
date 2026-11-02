use campfire_codec::Binary;
use campfire_items::ItemsDirective;
use campfire_orders::OrdersDirective;
use campfire_players::CapabilityDirective;
use campfire_production::ProductionDirective;
use campfire_progression::ProgressionDirective;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

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
    /// Whether its player carries it out, whatever units its order names, so its order goes to
    /// none, as a perk's learn.
    pub const fn is_players(&self) -> bool {
        matches!(self, Self::Progression(directive) if directive.is_players())
    }

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
    /// Each capability whose commands hold a directive, with the decoder of its directives.
    const DECODERS: [(Capability, DirectiveDecoder); 4] = [
        (OrdersDirective::CAPABILITY, Self::decode::<OrdersDirective>),
        (
            ProgressionDirective::CAPABILITY,
            Self::decode::<ProgressionDirective>,
        ),
        (
            ProductionDirective::CAPABILITY,
            Self::decode::<ProductionDirective>,
        ),
        (ItemsDirective::CAPABILITY, Self::decode::<ItemsDirective>),
    ];

    /// The decoder of `capability`'s directives, as its reader decodes the body of a command to
    /// it; none for a capability whose commands hold no directive.
    pub(crate) fn decoder(capability: Capability) -> Option<DirectiveDecoder> {
        Self::DECODERS
            .into_iter()
            .find(|&(of, _)| of == capability)
            .map(|(_, decoder)| decoder)
    }

    fn decode<D: CapabilityDirective + Into<Self>>(body: &[u8]) -> Option<Self> {
        D::decode(body).map(Into::into)
    }

    /// Appends it, the body of its order's command, to `body`.
    pub(crate) fn encode_into(self, body: &mut Vec<u8>) {
        match self {
            Self::Orders(directive) => Binary::encode_into(&directive, body),
            Self::Progression(directive) => Binary::encode_into(&directive, body),
            Self::Production(directive) => Binary::encode_into(&directive, body),
            Self::Items(directive) => Binary::encode_into(&directive, body),
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
