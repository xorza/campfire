use std::collections::BTreeMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;

use crate::stable_id::StableId;

/// Live entities by stable id, in id order: the order every hash and snapshot walks, without
/// sorting each tick. The `StableId` hooks keep it current. The first named ids, the match's and
/// its players', are held in an array by their place past `StableId::MATCH`, so the lookups of the
/// match's and the players' entities, which every pass makes, cost no search; any other id, a
/// restored snapshot's among them, is held in the tree.
#[derive(Resource, Debug, Default)]
pub struct EntityIndex {
    /// The entities of every id but the first named ones.
    entities: BTreeMap<StableId, Entity>,
    /// The entity of each of the first named ids, by its place past `StableId::MATCH`.
    named: Vec<Option<Entity>>,
}

impl EntityIndex {
    /// The named ids the array holds: the match's and those of 255 players.
    const NAMED: u64 = 256;

    pub fn get(&self, id: StableId) -> Option<Entity> {
        match Self::named_place(id) {
            Some(place) => self.named.get(place).copied().flatten(),
            None => self.entities.get(&id).copied(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (StableId, Entity)> + '_ {
        let pair = |(&id, &entity): (&StableId, &Entity)| (id, entity);
        let named = (self.named.iter().zip(StableId::MATCH.get()..))
            .filter_map(|(&entity, id)| Some((StableId::new(id), entity?)));
        (self.entities.range(..StableId::MATCH).map(pair))
            .chain(named)
            .chain(self.entities.range(StableId::MATCH..).map(pair))
    }

    /// The highest allocated id in use, past which every named id lies.
    pub(crate) fn last_allocated(&self) -> Option<StableId> {
        self.entities
            .range(..StableId::MATCH)
            .next_back()
            .map(|(&id, _)| id)
    }

    pub(crate) fn insert(&mut self, id: StableId, entity: Entity) -> Option<Entity> {
        let Some(place) = Self::named_place(id) else {
            return self.entities.insert(id, entity);
        };
        if self.named.len() <= place {
            self.named.resize(place + 1, None);
        }
        self.named[place].replace(entity)
    }

    pub(crate) fn remove(&mut self, id: StableId) -> Option<Entity> {
        match Self::named_place(id) {
            Some(place) => self.named.get_mut(place).and_then(Option::take),
            None => self.entities.remove(&id),
        }
    }

    /// The place past `StableId::MATCH` of `id`, one of the first named ids; none for another.
    fn named_place(id: StableId) -> Option<usize> {
        let place = id.get().checked_sub(StableId::MATCH.get())?;
        (place < Self::NAMED).then(|| usize::try_from(place).expect("a place under 256 fits usize"))
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;

    use crate::stable_id::StableId;
    use crate::stable_id::entity_index::EntityIndex;

    #[test]
    fn it_walks_every_id_in_order_and_finds_each() {
        let mut world = World::new();
        let mut entity = || world.spawn_empty().id();
        let match_id = StableId::MATCH;
        let player = StableId::new(StableId::MATCH.get() + 2);
        // Past the array's 256 named ids, as a restored snapshot may hold.
        let far = StableId::new(StableId::MATCH.get() + 300);
        let ids = [far, player, StableId::new(7), match_id, StableId::new(3)];
        let entities: Vec<Entity> = ids.iter().map(|_| entity()).collect();
        let mut index = EntityIndex::default();
        for (&id, &entity) in ids.iter().zip(&entities) {
            assert_eq!(index.insert(id, entity), None);
        }
        let order: Vec<u64> = index.iter().map(|(id, _)| id.get()).collect();
        let named = StableId::MATCH.get();
        assert_eq!(order, [3, 7, named, named + 2, named + 300]);
        for (&id, &entity) in ids.iter().zip(&entities) {
            assert_eq!(index.get(id), Some(entity));
        }
        assert_eq!(index.last_allocated(), Some(StableId::new(7)));
        assert_eq!(index.remove(player), Some(entities[1]));
        assert_eq!(index.get(player), None);
        assert_eq!(index.remove(far), Some(entities[0]));
        let order: Vec<u64> = index.iter().map(|(id, _)| id.get()).collect();
        assert_eq!(order, [3, 7, named]);
    }
}
