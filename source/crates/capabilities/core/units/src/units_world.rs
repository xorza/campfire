use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_sim::{SimWorld, StableId};
use campfire_values::{Allegiance, Relations, Team};

use crate::dead::Dead;
use crate::deaths::{Deaths, Fallen};
use crate::died::Died;
use crate::life::{self, Life};
use crate::owner::Owner;

/// What the units module gives a world, for every crate.
pub trait UnitsWorld {
    /// The match's entity, once it spawned.
    fn match_entity(&self) -> Option<Entity>;

    /// The relations of the match, on its entity, which every match spawns as it starts.
    fn relations(&self) -> &Relations;

    /// The relations of the match, to change.
    fn relations_mut(&mut self) -> Mut<'_, Relations>;

    /// The allegiance of `unit`, while it is in the match.
    fn allegiance_of(&self, unit: StableId) -> Option<Allegiance>;

    /// Takes `unit`'s life whole, when the sim simulates it and it lives: it dies as the next
    /// tick's deaths find it, with no killer. A predicting client takes none of a unit it holds
    /// as the server sent it, which `Unpredicted` hides.
    fn drain_life(&mut self, unit: StableId);

    /// Kills `unit`, a unit of no life that lives, as its last damage would kill one of life:
    /// it is dead, its death in the tick's record with `killer` its killer, and `Died` triggers,
    /// so its bounty, its hooks and its despawn follow as any death's.
    fn kill(&mut self, unit: StableId, killer: Option<StableId>);
}

impl UnitsWorld for World {
    fn match_entity(&self) -> Option<Entity> {
        self.entity_of(StableId::MATCH)
    }

    fn relations(&self) -> &Relations {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        self.get(entity)
            .expect("the match's entity holds the relations")
    }

    fn relations_mut(&mut self) -> Mut<'_, Relations> {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        self.get_mut(entity)
            .expect("the match's entity holds the relations")
    }

    fn allegiance_of(&self, unit: StableId) -> Option<Allegiance> {
        let entity = self.entity_of(unit)?;
        Some(Allegiance {
            team: *self.get::<Team>(entity)?,
            owner: self.get::<Owner>(entity).map(|owner| owner.player_slot()),
        })
    }

    fn drain_life(&mut self, unit: StableId) {
        let Some(entity) = self.queried_entity_of(unit) else {
            return;
        };
        let dead = self.get::<Dead>(entity).is_some();
        let Some(mut held) = self.get_mut::<Life>(entity) else {
            return;
        };
        if life::lives(dead, Some(&held)) {
            let all = held.current();
            held.take(all);
        }
    }

    fn kill(&mut self, unit: StableId, killer: Option<StableId>) {
        let Some(entity) = self.queried_entity_of(unit) else {
            return;
        };
        debug_assert!(
            self.get::<Life>(entity).is_none(),
            "a unit of life dies by its damage"
        );
        if self.get::<Dead>(entity).is_some() {
            return;
        }
        let fallen = Fallen::of(self.entity(entity));
        self.resource_mut::<Deaths>().push(fallen, killer, []);
        self.entity_mut(entity).insert(Dead);
        self.trigger(Died { id: unit });
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;

    use crate::player_units::{AvatarData, AvatarFilter, Commanded};

    /// What the units module gives a test's world.
    pub trait UnitsWorldInternals {
        /// The avatar of `player_slot`, as `PlayerUnits::avatar` finds it, for code that holds the
        /// world shared, as a test does: its query is made for each read.
        fn avatar_of(&self, player_slot: PlayerSlot) -> Option<Commanded>;
    }

    impl UnitsWorldInternals for World {
        fn avatar_of(&self, player_slot: PlayerSlot) -> Option<Commanded> {
            let mut avatars = self.try_query_filtered::<AvatarData, AvatarFilter>()?;
            Commanded::avatar_among(avatars.iter(self), player_slot)
        }
    }
}
