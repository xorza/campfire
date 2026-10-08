use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::geometry::region::Region;
use crate::items::item_id::ItemId;
use crate::players::resource_id::ResourceId;
use crate::units::team::Team;
use crate::values::share::Share;

/// The mode's shop as a match reads it: the item types it sells, the player resource it takes,
/// the places it stands, each a region of the map that serves one team, and the share of an
/// item's cost a sale gives back. Package data, not state.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shop {
    sells: Vec<ItemId>,
    pub(crate) resource: ResourceId,
    places: Vec<ShopPlace>,
    sell_share: Share,
}

/// A region of the map where the shop serves `team`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShopPlace {
    pub(crate) team: Team,
    pub(crate) region: Region,
}

impl Shop {
    pub(crate) const fn new(
        sells: Vec<ItemId>,
        resource: ResourceId,
        places: Vec<ShopPlace>,
        sell_share: Share,
    ) -> Shop {
        Shop {
            sells,
            resource,
            places,
            sell_share,
        }
    }

    pub(crate) fn sells(&self, item: ItemId) -> bool {
        self.sells.contains(&item)
    }

    /// Whether a unit of `team` at `pos` stands in one of the shop's places for its team.
    pub(crate) fn serves(&self, team: Team, pos: Position) -> bool {
        self.places
            .iter()
            .any(|place| place.team == team && place.region.contains(pos))
    }

    /// What a sale of `count` items of `cost` each gives back: the sell share of their cost,
    /// rounded down to a whole amount, exactly.
    pub(crate) fn refund(&self, cost: i64, count: u32) -> i64 {
        let whole = cost
            .checked_mul(i64::from(count))
            .expect("a stack's cost fits");
        self.sell_share.of(whole)
    }
}
