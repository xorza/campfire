use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::Allow;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_declarations::{ByType, Cache};
use campfire_sim::{StableId, Unpredicted};
use campfire_values::UnitType;

use crate::shop::Shop;

/// The units whose type has a shop, living or dead, by stable id, as a search for the nearest
/// shop visits them: the install takes each one its world holds, and its observers then add each
/// one as its type comes, and drop it as its type goes, as each spawn, restore and replication
/// inserts a unit's type, which never changes in place. Derived, never state.
#[derive(Resource, Debug, Default)]
pub(crate) struct ShopIndex {
    units: Vec<ShopUnit>,
}

/// A unit whose type has a shop.
#[derive(Debug, Clone, Copy)]
struct ShopUnit {
    id: StableId,
    entity: Entity,
}

impl ShopIndex {
    /// Each unit whose type has a shop, by stable id.
    pub(crate) fn entities(&self) -> impl ExactSizeIterator<Item = Entity> + '_ {
        self.units.iter().map(|unit| unit.entity)
    }

    /// Adds a unit whose type, inserted, has a shop.
    pub(crate) fn typed(
        inserted: On<'_, '_, Insert<UnitType>>,
        units: Query<'_, '_, (&StableId, &UnitType), Allow<Unpredicted>>,
        shops: Res<'_, ByType<Shop>>,
        mut index: ResMut<'_, Self>,
    ) {
        let entity = inserted.entity;
        index.units.retain(|unit| unit.entity != entity);
        let Ok((&id, &unit_type)) = units.get(entity) else {
            return;
        };
        if shops.get(unit_type).is_some() {
            let at = index.units.partition_point(|unit| unit.id < id);
            index.units.insert(at, ShopUnit { id, entity });
        }
    }

    /// Drops a unit whose type goes, as it despawns.
    pub(crate) fn untyped(removed: On<'_, '_, Remove<UnitType>>, mut index: ResMut<'_, Self>) {
        let entity = removed.entity;
        index.units.retain(|unit| unit.entity != entity);
    }
}

impl Cache for ShopIndex {
    /// Each unit of `world` whose type has a shop, as a test's units spawn before its install.
    fn installed(world: &World) -> Option<Self> {
        let mut index = Self::default();
        let shops = world.get_resource::<ByType<Shop>>();
        let units =
            world.try_query_filtered::<(Entity, &StableId, &UnitType), Allow<Unpredicted>>();
        if let (Some(shops), Some(mut units)) = (shops, units) {
            index.units.extend(
                units
                    .iter(world)
                    .filter(|&(.., &unit_type)| shops.get(unit_type).is_some())
                    .map(|(entity, &id, _)| ShopUnit { id, entity }),
            );
            index.units.sort_unstable_by_key(|unit| unit.id);
        }
        Some(index)
    }
}
