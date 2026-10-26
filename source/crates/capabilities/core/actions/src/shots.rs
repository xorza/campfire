use bevy_ecs::resource::Resource;
use campfire_declarations::{Role, Scratch};
use campfire_math::Vec3;
use campfire_sim::StableId;
use campfire_values::UnitType;

use crate::action_target::ActionTarget;
use crate::shot_damage::ShotDamage;

/// The projectiles the tick's ranged attacks fire, delivery requests that `projectiles` launches at
/// `ActionsSystems::Fire`, in the order they were fired, which no reader changes; combat gives a
/// shot its barrel's launch before then. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct Shots(Vec<Shot>);

/// The projectile of a ranged attack: of `unit_type`, from `source` at `from`, leaving its
/// barrel as `launch` says when its weapon has one, at `target`, a unit or, for a shot that
/// scatters, a point of the ground, with its `damage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub source: StableId,
    pub from: Vec3,
    pub launch: Option<ShotLaunch>,
    pub target: ActionTarget,
    pub unit_type: UnitType,
    pub damage: ShotDamage,
}

/// Where a shot leaves its barrel (04-capabilities/combat.md, A shot leaves its weapon's barrel):
/// the point in space its projectile starts at, and the way it leaves along, a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotLaunch {
    pub at: Vec3,
    pub way: Vec3,
}

impl Shots {
    /// Adds `shot`, fired after every shot it holds.
    pub fn fire(&mut self, shot: Shot) {
        self.0.push(shot);
    }

    /// Each shot, in the order they were fired, to change.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Shot> {
        self.0.iter_mut()
    }

    /// Takes every shot, in the order they were fired.
    pub fn drain(&mut self) -> impl Iterator<Item = Shot> + '_ {
        self.0.drain(..)
    }
}

impl Scratch for Shots {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
