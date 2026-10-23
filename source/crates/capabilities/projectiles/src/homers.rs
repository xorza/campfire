use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_sim::StableId;

/// The projectiles that home on a unit or are steered tracking it, by that unit's stable id,
/// then by entity, as `Projectile`'s hooks keep them, so a death or a teleport visits only the
/// projectiles of its unit. A cache, not state: a restore inserts each projectile again.
#[derive(Resource, Debug, Default)]
pub(crate) struct Homers(Vec<Homer>);

/// A projectile and the unit it homes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Homer {
    target: StableId,
    projectile: Entity,
}

impl Cache for Homers {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

impl Homers {
    /// The projectiles that home on `target`, by entity.
    pub(crate) fn of(&self, target: StableId) -> impl Iterator<Item = Entity> + '_ {
        let first = self.0.partition_point(|homer| homer.target < target);
        self.0[first..]
            .iter()
            .take_while(move |homer| homer.target == target)
            .map(|homer| homer.projectile)
    }

    /// Adds `projectile`, which homes on `target`.
    pub(crate) fn add(&mut self, target: StableId, projectile: Entity) {
        let homer = Homer { target, projectile };
        if let Err(at) = self.0.binary_search(&homer) {
            self.0.insert(at, homer);
        }
    }

    /// Takes out `projectile`, which homed on `target`.
    pub(crate) fn remove(&mut self, target: StableId, projectile: Entity) {
        if let Ok(at) = self.0.binary_search(&Homer { target, projectile }) {
            self.0.remove(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn the_homers_of_a_unit_are_its_projectiles_alone() {
        let mut ids = IdAllocator::default();
        let [a, b] = [(); 2].map(|()| ids.allocate());
        let [first, second, third] = [1, 2, 3].map(|index| Entity::from_raw_u32(index).unwrap());
        let mut homers = Homers::default();
        // Two on `a`, added out of order and one twice, and one on `b`.
        for (target, projectile) in [(a, second), (b, third), (a, first), (a, second)] {
            homers.add(target, projectile);
        }
        let of = |homers: &Homers, target| homers.of(target).collect::<Vec<_>>();
        let mut on_a = vec![first, second];
        on_a.sort_unstable();
        assert_eq!((of(&homers, a), of(&homers, b)), (on_a, vec![third]));
        homers.remove(a, first);
        homers.remove(b, first);
        assert_eq!(
            (of(&homers, a), of(&homers, b)),
            (vec![second], vec![third])
        );
    }
}
