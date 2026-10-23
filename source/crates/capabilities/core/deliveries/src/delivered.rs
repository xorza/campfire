use campfire_actions::{Reached, ShotDamage};
use campfire_sim::StableId;
use campfire_values::{Allegiance, Hit};

use crate::delivering::Delivering;

/// What a delivery carrying `payload` reached, as `hit` says: a unit it hit, whose hook is
/// `on_hit`, or its end, whose hook is `on_end`. One record for every delivery, which the hooks
/// run for an action's and combat answers for an attack's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivered {
    pub payload: DeliveredPayload,
    pub reach: Reached,
    pub hit: Hit,
}

/// What a delivery carries: what an action's delivery belongs to, whose hooks and lists its hits
/// and its end run, or a ranged attack's shot, whose damage combat deals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveredPayload {
    Action(Delivering),
    Attack(AttackShot),
}

/// A ranged attack's shot: `source`'s, of `allegiance` as it fired, with its `damage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackShot {
    pub source: StableId,
    pub allegiance: Allegiance,
    pub damage: ShotDamage,
}
