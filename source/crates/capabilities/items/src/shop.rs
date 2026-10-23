use campfire_declarations::PerId;
use campfire_math::Num;
use campfire_script::Filter;
use campfire_values::{ItemId, Relation, ResourceId, Share, TagSet, UnitType};

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
