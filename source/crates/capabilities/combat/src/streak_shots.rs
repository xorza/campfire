use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_common::Tick;
use campfire_declarations::{Role, Scratch};
use campfire_sim::StableId;
use campfire_values::ActionId;

/// The shots of streak weapons this tick's strikes fired, in their order, which the server's
/// count then takes into each unit's streak (04-capabilities/combat.md, The streak is the
/// server's). Not state: the count takes it in the strikes' set.
#[derive(Resource, Debug, Default)]
pub(crate) struct StreakShots(Vec<FiredShot>);

/// A shot a unit, by stable id and entity, fired of a streak weapon.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FiredShot {
    pub(crate) unit: StableId,
    pub(crate) entity: Entity,
    pub(crate) shot: StreakShot,
}

/// A shot of a streak weapon: the weapon, its target, the tick it went off, and the tick the
/// weapon is ready again.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StreakShot {
    pub(crate) weapon: ActionId,
    pub(crate) target: StableId,
    pub(crate) now: Tick,
    pub(crate) ready: Tick,
}

impl StreakShots {
    pub(crate) fn push(&mut self, fired: FiredShot) {
        self.0.push(fired);
    }

    /// Takes the shots, as the count visits them.
    pub(crate) fn take(&mut self, into: &mut Vec<FiredShot>) {
        into.clear();
        into.append(&mut self.0);
    }
}

impl Scratch for StreakShots {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
