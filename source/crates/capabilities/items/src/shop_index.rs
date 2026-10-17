use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::Allow;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_declare::{ByType, Cache};
use campfire_sim::{StableId, Unpredicted};
use campfire_values::UnitType;

use crate::shop::Shop;

/// The units whose type has a shop, living or dead, by stable id, as a search for the nearest
/// shop visits them: its observers add each one as its type comes, and drop it as its type goes,
/// as each spawn, restore and replication inserts a unit's type, which never changes in place.
/// The first search takes every such unit the world holds, as the observers install after a
/// test's first units spawn. Derived, never state.
#[derive(Resource, Debug, Default)]
pub(crate) struct ShopIndex {
    units: Vec<ShopUnit>,
    taken: bool,
}

/// A unit whose type has a shop.
#[derive(Debug, Clone, Copy)]
struct ShopUnit {
    id: StableId,
    entity: Entity,
}

/// Each unit's stable id and type, as the index reads them, a client's held units among them.
pub(crate) type TypedUnits<'w, 's> =
    Query<'w, 's, (Entity, &'static StableId, &'static UnitType), Allow<Unpredicted>>;

impl ShopIndex {
    /// Takes every unit of `units` whose type `shops` gives a shop, once.
    pub(crate) fn take(&mut self, units: &TypedUnits<'_, '_>, shops: &ByType<Shop>) {
        if self.taken {
            return;
        }
        self.units.clear();
        self.units.extend(
            units
                .iter()
                .filter(|&(.., &unit_type)| shops.get(unit_type).is_some())
                .map(|(entity, &id, _)| ShopUnit { id, entity }),
        );
        self.units.sort_unstable_by_key(|unit| unit.id);
        self.taken = true;
    }

    /// Each unit whose type has a shop, by stable id.
    pub(crate) fn entities(&self) -> impl ExactSizeIterator<Item = Entity> + '_ {
        debug_assert!(self.taken, "a search takes the shop units first");
        self.units.iter().map(|unit| unit.entity)
    }

    /// Adds a unit whose type, inserted, has a shop.
    pub(crate) fn typed(
        inserted: On<'_, '_, Insert, UnitType>,
        units: TypedUnits<'_, '_>,
        shops: Res<'_, ByType<Shop>>,
        mut index: ResMut<'_, Self>,
    ) {
        let entity = inserted.entity;
        index.units.retain(|unit| unit.entity != entity);
        let Ok((_, &id, &unit_type)) = units.get(entity) else {
            return;
        };
        if index.taken && shops.get(unit_type).is_some() {
            let at = index.units.partition_point(|unit| unit.id < id);
            index.units.insert(at, ShopUnit { id, entity });
        }
    }

    /// Drops a unit whose type goes, as it despawns.
    pub(crate) fn untyped(removed: On<'_, '_, Remove, UnitType>, mut index: ResMut<'_, Self>) {
        let entity = removed.entity;
        index.units.retain(|unit| unit.entity != entity);
    }
}

impl Cache for ShopIndex {
    fn installed() -> Option<Self> {
        Some(Self::default())
    }
}
