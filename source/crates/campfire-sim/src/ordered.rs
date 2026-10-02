use bevy_ecs::entity::Entity;

use crate::stable_id::StableId;

/// A system's scratch that puts the entities it concerns in stable-id order. A query gives its
/// entities in archetype order, which a component added to or removed from one unit, or a
/// restore, changes; so a system that spends something shared, takes ids or runs scripts walks
/// them through this. Kept in a `Local`, it allocates nothing once its system ran with as many
/// entities.
#[derive(Debug, Default)]
pub struct Ordered {
    entities: Vec<Keyed>,
}

/// An entity and its stable id, which orders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Keyed {
    pub id: StableId,
    pub entity: Entity,
}

impl Ordered {
    /// `entities` in stable-id order. Each id is one entity's, so the order is total.
    pub fn sort(&mut self, entities: impl IntoIterator<Item = Keyed>) -> &[Keyed] {
        self.entities.clear();
        self.entities.extend(entities);
        self.entities.sort_unstable_by_key(|keyed| keyed.id);
        debug_assert!(
            self.entities.windows(2).all(|pair| pair[0].id < pair[1].id),
            "each stable id is one entity's"
        );
        &self.entities
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;

    #[test]
    fn entities_come_in_stable_id_order_whatever_order_they_came_in() {
        let mut world = World::new();
        let [a, b, c] = [(); 3].map(|()| world.spawn_empty().id());
        let keyed = |id, entity| Keyed {
            id: StableId::new(id),
            entity,
        };
        let mut ordered = Ordered::default();
        let sorted = ordered.sort([keyed(7, a), keyed(2, b), keyed(5, c)]);
        assert_eq!(sorted, [keyed(2, b), keyed(5, c), keyed(7, a)]);
        // A second sort keeps nothing of the first.
        assert_eq!(ordered.sort([keyed(3, c)]), [keyed(3, c)]);
        assert_eq!(ordered.sort([]), []);
    }
}
