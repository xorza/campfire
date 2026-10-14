use bevy_ecs::world::{EntityRef, World};
use campfire_progression::Experience;
use campfire_sim::{EntityIndex, Position, SimWorld, StableId};
use campfire_units::{Owner, SpawnPoint};
use campfire_values::Team;

/// The units of a running match, as a scripted player or a test names them by role.
#[derive(Debug, Clone, Copy)]
pub struct MatchUnits<'a> {
    world: &'a World,
}

impl<'a> MatchUnits<'a> {
    /// The units of `world`, a running match.
    pub const fn of(world: &'a World) -> MatchUnits<'a> {
        MatchUnits { world }
    }

    pub const fn world(self) -> &'a World {
        self.world
    }

    /// Every unit, with its stable id, in the order of the ids.
    pub fn all(self) -> impl Iterator<Item = (StableId, EntityRef<'a>)> {
        let world = self.world;
        world
            .resource::<EntityIndex>()
            .iter()
            .map(move |(id, entity)| (id, world.entity(entity)))
    }

    /// Player `slot`'s hero: the unit it owns that gains experience.
    pub fn hero(self, slot: u32) -> StableId {
        let mut heroes = self.heroes(slot);
        heroes.next().expect("each player has a hero")
    }

    /// The units player `slot` owns that gain experience, by stable id.
    pub fn heroes(self, slot: u32) -> impl Iterator<Item = StableId> {
        self.all()
            .filter(move |(_, unit)| {
                MatchUnits::owned_by(unit, slot) && unit.contains::<Experience>()
            })
            .map(|(id, _)| id)
    }

    pub fn position(self, id: StableId) -> Position {
        let entity = self.world.entity_of(id).expect("a unit of the match");
        *self
            .world
            .get::<Position>(entity)
            .expect("a unit stands somewhere")
    }

    /// The units that stand at `at`, by stable id.
    pub fn standing_at(self, at: impl Into<Position>) -> Vec<StableId> {
        let at = at.into();
        self.all()
            .filter(|(_, unit)| unit.get::<Position>() == Some(&at))
            .map(|(id, _)| id)
            .collect()
    }

    /// Whether player `slot` owns `unit`.
    pub fn owned_by(unit: &EntityRef<'_>, slot: u32) -> bool {
        unit.get::<Owner>()
            .is_some_and(|owner| owner.slot().get() == slot)
    }

    /// The unit that spawned at `at`, of a team: a camp's, or a structure.
    pub fn spawned_at(self, at: impl Into<Position>) -> StableId {
        let at = at.into();
        self.all()
            .find(|(_, unit)| {
                unit.contains::<Team>()
                    && unit.get::<SpawnPoint>().map(|point| point.get()) == Some(at)
            })
            .map_or_else(|| panic!("a unit spawned at {at:?}"), |(id, _)| id)
    }
}
