use bevy_ecs::query::{Allow, Has, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_declare::ByType;
use campfire_players::OnMatch;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Body, Dead, MapBook};
use campfire_values::{Relations, TagSet, Team, UnitTags, UnitType};

use crate::inventory::{Inventory, Purchase};
use crate::item_book::ItemBook;
use crate::item_id::ItemId;
use crate::shop::{Buyer, Shop};

/// The shops as a trade and a client's shop panel read them: the shop of a living shop unit,
/// whether it serves a unit now, and what an item costs a carrier there. A client reads the units
/// it holds, the server's among them.
#[derive(SystemParam, Debug)]
pub struct Shopping<'w, 's> {
    index: Res<'w, EntityIndex>,
    items: Res<'w, ItemBook>,
    shops: Res<'w, ByType<Shop>>,
    relations: OnMatch<'w, 's, Relations>,
    map: Res<'w, MapBook>,
    shop_units: ShopUnits<'w, 's>,
    visitors: Visitors<'w, 's>,
}

/// A living shop unit's type, team, place and body.
type ShopUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static UnitType,
        &'static Team,
        &'static Position,
        Option<&'static Body>,
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

/// A unit a shop may serve: its team, place, body and tags, and whether it is dead.
type Visitors<'w, 's> = Query<
    'w,
    's,
    (
        &'static Team,
        &'static Position,
        Option<&'static Body>,
        Option<&'static UnitTags>,
        Has<Dead>,
    ),
    Allow<Unpredicted>,
>;

impl Shopping<'_, '_> {
    /// The shop of the living unit `shop`, when it is one.
    pub fn shop(&self, shop: StableId) -> Option<&Shop> {
        let (&unit_type, ..) = self.shop_units.get(self.index.get(shop)?).ok()?;
        self.shops.get(unit_type)
    }

    /// The shop of the living unit `shop`, when it is one and serves the unit `unit` now: its
    /// buyers select the unit, as the shop's team regards the unit's, `dead` among its tags while
    /// it is dead; and, unless it is dead, its reach meets the unit from the shop's edge.
    pub fn serving(&self, shop: StableId, unit: StableId) -> Option<&Shop> {
        let (&unit_type, &team, &at, body) = self.shop_units.get(self.index.get(shop)?).ok()?;
        let spec = self.shops.get(unit_type)?;
        let (&visitor_team, &pos, visitor_body, tags, dead) =
            self.visitors.get(self.index.get(unit)?).ok()?;
        let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
        let buyer = Buyer {
            relation: self.relations.get().between(team, visitor_team),
            tags: Dead::tagged(tags, dead),
            dead,
        };
        spec.serves(buyer, |reach| {
            let (from, to) = (Body::shape_of(body), Body::shape_of(visitor_body));
            self.map.metric.reaches(at, from, reach, pos, to)
        })
        .then_some(spec)
    }

    /// What buying `item` at `shop` costs a carrier of `inventory`, and whether it has room for
    /// it, `given_up` counting the components it gives up slot by slot; none for an item the shop
    /// does not sell.
    pub fn purchase(
        &self,
        shop: &Shop,
        inventory: &Inventory,
        item: ItemId,
        given_up: &mut Vec<u32>,
    ) -> Option<Purchase> {
        shop.sells(item)
            .then(|| inventory.purchase(&self.items, item, shop.resource(), given_up))
    }
}
