use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::{Deserialize, Serialize};

/// How an action started: where its unit stood, and, for a charged action, the share of its most
/// it charged, from 0 to 1. Every hook of the action reads them, its delivery's included, as
/// `ctx.origin` and `ctx.charge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionStart {
    pub origin: Vec3,
    pub charge: Option<Num>,
}

impl ActionStart {
    /// Whether each of its places lies within the world's bound, as a restore checks it.
    pub fn within_bound(self) -> bool {
        Position::from(self.origin).within_bound()
    }
}
