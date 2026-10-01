use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::actions::action_book::ActionId;
use crate::projectiles::hit::Hit;
use crate::scripts::hook::Hook;

/// The hits and ends of this tick's flights whose action's hooks run, in the order of their
/// projectiles' stable ids, then the order they happened; and the projectiles that ended, which
/// despawn once the hooks ran, so a hook still reads its delivery. Not state: it empties within
/// the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Deliveries {
    pub(crate) delivered: Vec<Delivered>,
    pub(crate) ended: Vec<Entity>,
}

/// A hit of the unit `reached`, `on_hit`, or an end, `on_end`, of a projectile of `source`
/// carrying `action` at `rank`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Delivered {
    pub(crate) source: StableId,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
    pub(crate) hook: Hook,
    pub(crate) reached: Option<StableId>,
    pub(crate) hit: Hit,
}
