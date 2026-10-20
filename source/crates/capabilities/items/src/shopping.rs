use bevy_ecs::query::{Allow, Has, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_declarations::ByType;
use campfire_geometry::Placed;
use campfire_math::Vec3;
use campfire_players::OnMatch;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Body, Dead, MapBook};
use campfire_values::{ItemId, Relation, Relations, TagSet, Team, UnitTags, UnitType};

use crate::inventory::{Inventory, Purchase};
use crate::item_book::ItemBook;
use crate::shop::{Buyer, Shop};
use crate::shop_index::ShopIndex;

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
    shop_index: Res<'w, ShopIndex>,
}

/// A living shop unit's stable id, type, team, place and body.
type ShopUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
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

/// A unit as a shop meets it: its team, place and body, and its tags as it stands now.
#[derive(Debug, Clone, Copy)]
struct Visitor<'a> {
    team: Team,
    position: Vec3,
    body: Option<&'a Body>,
    tags: TagSet,
    dead: bool,
}

impl Visitor<'_> {
    /// It as a buyer of a shop whose team regards its team with `relation`.
    const fn buyer(&self, relation: Relation) -> Buyer {
        Buyer {
            relation,
            tags: self.tags,
            dead: self.dead,
        }
    }
}

impl Shopping<'_, '_> {
    /// The shop of the living unit `shop`, when it is one.
    pub fn shop(&self, shop: StableId) -> Option<&Shop> {
        let (_, &unit_type, ..) = self.shop_units.get(self.index.get(shop)?).ok()?;
        self.shops.get(unit_type)
    }

    /// The shop of the living unit `shop`, when it is one and serves the unit `unit` now: its
    /// buyers select the unit, as the shop's team regards the unit's, `dead` among its tags while
    /// it is dead; and, unless it is dead, its reach meets the unit from the shop's edge.
    pub fn serving(&self, shop: StableId, unit: StableId) -> Option<&Shop> {
        let (_, &unit_type, &team, &at, body) = self.shop_units.get(self.index.get(shop)?).ok()?;
        let spec = self.shops.get(unit_type)?;
        let visitor = self.visitor(unit)?;
        let buyer = visitor.buyer(self.relations.get().between(team, visitor.team));
        spec.serves(buyer, |reach| {
            let from = Placed::new(at.get(), Body::shape_of(body));
            let to = Placed::new(visitor.position, Body::shape_of(visitor.body));
            self.map.metric.reaches(from, reach, to)
        })
        .then_some(spec)
    }

    /// The nearest living shop unit whose buyers select the unit `unit`, in its reach or not:
    /// by the distance from the unit to the nearest point of the shop's body, the lower stable id
    /// on a tie, as a shop panel shows it. It visits only the units whose type has a shop.
    pub fn nearest_for(&self, unit: StableId) -> Option<StableId> {
        let visitor = self.visitor(unit)?;
        let relations = self.relations.get();
        let shop_units = self.shop_index.entities();
        let selecting = shop_units
            .filter_map(|entity| self.shop_units.get(entity).ok())
            .filter_map(|(&id, &unit_type, &team, &at, body)| {
                let spec = self.shops.get(unit_type)?;
                let buyer = visitor.buyer(relations.between(team, visitor.team));
                spec.selects(buyer).then(|| {
                    let nearest = Body::shape_of(body).nearest_point(at.get(), visitor.position);
                    let away = self.map.metric.offset(nearest, visitor.position);
                    (away.length_squared_bits(), id)
                })
            });
        selecting.min().map(|(_, id)| id)
    }

    /// The unit `unit` as a shop meets it.
    fn visitor(&self, unit: StableId) -> Option<Visitor<'_>> {
        let (&team, &position, body, tags, dead) = self.visitors.get(self.index.get(unit)?).ok()?;
        let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
        Some(Visitor {
            team,
            position: position.get(),
            body,
            tags: Dead::tagged(tags, dead),
            dead,
        })
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
