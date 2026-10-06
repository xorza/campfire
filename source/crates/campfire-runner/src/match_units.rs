use bevy_ecs::world::{EntityRef, World};
use campfire_capabilities::{Experience, Owner, SpawnPoint, Team};
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, Position, StableId};

use crate::fixed_match::FixedMatch;

/// The units of a running match, as a scripted player or a test names them by role.
#[derive(Debug, Clone, Copy)]
pub struct MatchUnits<'a> {
    world: &'a World,
}

impl<'a> MatchUnits<'a> {
    pub const fn of(fixed: &'a FixedMatch) -> MatchUnits<'a> {
        MatchUnits::of_world(fixed.runner().world())
    }

    pub const fn of_world(world: &'a World) -> MatchUnits<'a> {
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
        let entity = self
            .world
            .resource::<EntityIndex>()
            .get(id)
            .expect("a unit of the match");
        *self
            .world
            .get::<Position>(entity)
            .expect("a unit stands somewhere")
    }

    /// Whether player `slot` owns `unit`.
    pub fn owned_by(unit: &EntityRef<'_>, slot: u32) -> bool {
        unit.get::<Owner>()
            .is_some_and(|owner| owner.slot().get() == slot)
    }

    /// The unit that spawned at `x`, `z` on the ground, of a team: a camp's, or a structure.
    pub fn spawned_at(self, x: i64, z: i64) -> StableId {
        let at =
            Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).expect("a map point");
        self.all()
            .find(|(_, unit)| {
                unit.contains::<Team>()
                    && unit.get::<SpawnPoint>().map(|point| point.get()) == Some(at)
            })
            .map_or_else(|| panic!("a unit spawned at {x}, {z}"), |(id, _)| id)
    }
}
