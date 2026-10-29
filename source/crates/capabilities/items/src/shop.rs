use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::Num;
use campfire_names::{Filter, UnknownName};
use campfire_units::UnitTypes;
use campfire_values::{ItemId, Named, Names, Relation, ResourceId, Share, TagSet, UnitType};

use crate::Items;
use crate::item_book::ItemBook;
use crate::shop_data::ShopData;

/// A shop type's shop as a match reads it, package data and not state: the item types it sells,
/// the player resource it takes, how far from its body's edge it serves, the units it serves,
/// and the share of an item's cost a sale gives back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shop {
    sells: Vec<ItemId>,
    resource: ResourceId,
    reach: Num,
    buyers: Filter,
    sell_share: Share,
}

/// What a shop's test of a buyer reads of it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Buyer {
    /// How the shop's team regards the buyer's.
    pub(crate) relation: Relation,
    /// Its tags as it stands now, `dead` among them while it is dead.
    pub(crate) tags: TagSet,
    pub(crate) dead: bool,
}

impl Shop {
    pub const fn new(
        sells: Vec<ItemId>,
        resource: ResourceId,
        reach: Num,
        buyers: Filter,
        sell_share: Share,
    ) -> Self {
        Self {
            sells,
            resource,
            reach,
            buyers,
            sell_share,
        }
    }

    /// The item types it sells, in its data's order.
    pub fn items(&self) -> &[ItemId] {
        &self.sells
    }

    /// The player resource it takes and gives back.
    pub const fn resource(&self) -> ResourceId {
        self.resource
    }

    pub(crate) fn sells(&self, item: ItemId) -> bool {
        self.sells.contains(&item)
    }

    /// Whether its buyers select `buyer`, in its reach or not.
    pub(crate) const fn selects(&self, buyer: Buyer) -> bool {
        self.buyers.selects(buyer.relation, buyer.tags)
    }

    /// Whether it serves `buyer`: a unit its buyers select, dead from anywhere, as a dead unit
    /// stands nowhere, else one that `reaches` says its reach meets from the shop's edge.
    pub(crate) fn serves(&self, buyer: Buyer, reaches: impl FnOnce(Num) -> bool) -> bool {
        self.selects(buyer) && (buyer.dead || reaches(self.reach))
    }

    /// What a sale of `count` items of `cost` each gives back, no more than a stack of the item:
    /// the sell share of their cost, rounded down to a whole amount, exactly.
    pub(crate) fn refund(&self, cost: i64, count: u32) -> i64 {
        let whole = cost
            .checked_mul(i64::from(count))
            .expect("the item book bounds a full stack's cost");
        self.sell_share.of(whole)
    }
}

impl PerId<UnitType> for Shop {
    const SCRIPTS_ONLY: bool = false;
}

impl Shop {
    /// The shop of each unit type that stands and sells, of the mode's item types and resources;
    /// an error for a name the mode does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let items = cx.book::<ItemBook>()?.names();
        let no_resources = Names::default();
        let resources = Items::resources(cx, &no_resources)?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<ShopData>() else {
                continue;
            };
            let shop = || {
                let sells = data.items.iter().map(|name| {
                    let found = items.named(name.as_str());
                    found.ok_or_else(|| UnknownName::new(ItemId::NAMES, name))
                });
                let sells = sells.collect::<Result<_, UnknownName>>()?;
                let resource = resources.named(data.resource.as_str());
                let resource =
                    resource.ok_or_else(|| UnknownName::new(ResourceId::NAMES, &data.resource))?;
                let buyers = Filter::resolve(&data.buyers, tags)?;
                Ok(Self::new(
                    sells,
                    resource,
                    data.reach,
                    buyers,
                    data.sell_share,
                ))
            };
            let at = || BookPlace::UnitType(unit.place());
            let shop = shop().map_err(|name| BookError::unknown(unit.package, at(), name))?;
            book.set(unit.unit_type, shop);
        }
        Ok(book.build())
    }
}
