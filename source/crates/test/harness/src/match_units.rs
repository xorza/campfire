use bevy_ecs::world::{EntityRef, World};
use campfire_declare::ByType;
use campfire_items::Shop;
use campfire_math::Vec3;
use campfire_progression::Experience;
use campfire_sim::{EntityIndex, Position, SimWorld, StableId};
use campfire_units::{Owner, SpawnPoint};
use campfire_values::{Team, UnitType};

/// The units of a running match, as a scripted player or a test names them by role.
#[derive(Debug, Clone, Copy)]
pub struct MatchUnits<'a> {
    world: &'a World,
}

impl<'a> MatchUnits<'a> {
    /// The units of `world`, a running match.
    pub const fn of(world: &'a World) -> Self {
        MatchUnits { world }
    }

    pub const fn world(self) -> &'a World {
        self.world
    }

    /// Every unit, with its stable id, in the order of the ids.
    pub fn all(self) -> impl Iterator<Item = (StableId, EntityRef<'a>)> {
        self.world
            .resource::<EntityIndex>()
            .iter()
            .map(move |(id, entity)| (id, self.world.entity(entity)))
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

    /// The unit `id`, which the match holds.
    pub fn unit(self, id: StableId) -> EntityRef<'a> {
        let entity = self.world.entity_of(id).expect("a unit of the match");
        self.world.entity(entity)
    }

    pub fn pos(self, id: StableId) -> Vec3 {
        self.unit(id)
            .get::<Position>()
            .expect("a unit stands somewhere")
            .get()
    }

    /// The units that stand at `at`, by stable id.
    pub fn standing_at(self, at: impl Into<Vec3>) -> Vec<StableId> {
        let at = at.into();
        self.all()
            .filter(|(_, unit)| unit.get::<Position>().map(|pos| pos.get()) == Some(at))
            .map(|(id, _)| id)
            .collect()
    }

    /// The first shop unit of `unit`'s team, by stable id.
    pub fn shop_of(self, unit: StableId) -> StableId {
        let team = self.unit(unit).get::<Team>().expect("a unit of a team");
        let shops = self.world.resource::<ByType<Shop>>();
        self.all()
            .find(|(_, other)| {
                other.get::<Team>() == Some(team)
                    && other
                        .get::<UnitType>()
                        .is_some_and(|&unit_type| shops.get(unit_type).is_some())
            })
            .map(|(id, _)| id)
            .expect("a team with a shop")
    }

    /// Whether player `slot` owns `unit`.
    pub fn owned_by(unit: &EntityRef<'_>, slot: u32) -> bool {
        unit.get::<Owner>()
            .is_some_and(|owner| owner.slot().get() == slot)
    }

    /// The unit that spawned at `at`, of a team: a camp's, or a structure.
    pub fn spawned_at(self, at: impl Into<Vec3>) -> StableId {
        let at = at.into();
        self.all()
            .find(|(_, unit)| {
                unit.contains::<Team>()
                    && unit.get::<SpawnPoint>().map(|point| point.get()) == Some(at)
            })
            .map_or_else(|| panic!("a unit spawned at {at:?}"), |(id, _)| id)
    }
}
