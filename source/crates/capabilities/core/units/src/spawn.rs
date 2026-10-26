use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_sim::{IdAllocator, StableId};
use campfire_values::{Team, UnitType};

use crate::spawn_at::SpawnAt;
use crate::spawner::Spawner;
use crate::units::Units;

/// A test's unit to spawn, of a type the match loaded, at a place: on team 0, of no owner, facing 0
/// degrees, with no part of the caller's, unless its setters say otherwise. `place` spawns it with
/// the next stable id, as the mode's own spawns do, on any match, as a test sets a client's up with
/// the units the server would send.
#[derive(Debug, Clone, Copy)]
pub struct Spawn<P = ()> {
    unit_type: UnitType,
    position: Vec3,
    team: Team,
    owner: Option<PlayerSlot>,
    angle: Num,
    parts: P,
}

impl Spawn {
    /// A unit of `unit_type` at `position`.
    pub fn of(unit_type: UnitType, position: impl Into<Vec3>) -> Self {
        Self {
            unit_type,
            position: position.into(),
            team: Team::new(0),
            owner: None,
            angle: Num::ZERO,
            parts: (),
        }
    }

    /// A unit of the type `name` of `world`'s match at `position`.
    pub fn named(world: &World, name: &str, position: impl Into<Vec3>) -> Self {
        let unit_type =
            Units::type_named(world, name).unwrap_or_else(|| panic!("no unit type {name}"));
        Self::of(unit_type, position)
    }
}

impl<P: Bundle<Effect: NoBundleEffect>> Spawn<P> {
    /// On `team`.
    #[must_use]
    pub const fn team(mut self, team: Team) -> Self {
        self.team = team;
        self
    }

    /// Owned by the player of `slot`.
    #[must_use]
    pub const fn owner(mut self, slot: PlayerSlot) -> Self {
        self.owner = Some(slot);
        self
    }

    /// Turned `angle` degrees.
    #[must_use]
    pub const fn angle(mut self, angle: Num) -> Self {
        self.angle = angle;
        self
    }

    /// With `parts` of the caller's.
    pub fn parts<Q: Bundle<Effect: NoBundleEffect>>(self, parts: Q) -> Spawn<Q> {
        Spawn {
            unit_type: self.unit_type,
            position: self.position,
            team: self.team,
            owner: self.owner,
            angle: self.angle,
            parts,
        }
    }

    /// Spawns the unit in `world`, once the match loaded its types: the first spawn seals their
    /// tags, with no effect for a tag, as a mode's load does once its packages load.
    pub fn place(self, world: &mut World) -> StableId {
        Units::seal_once(world);
        let id = world.resource_mut::<IdAllocator>().allocate();
        let at = SpawnAt {
            id,
            unit_type: self.unit_type,
            team: self.team,
            owner: self.owner,
            position: self.position,
            angle: self.angle,
            variation: None,
        };
        Spawner::place_any(world, at, self.parts);
        id
    }
}
