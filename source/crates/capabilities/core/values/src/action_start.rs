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
    /// Whether it holds what a start can, as a restore checks it: its origin within the world's
    /// bound, and its charge, when it has one, from 0 to 1.
    pub fn check(self) -> bool {
        let charge = self
            .charge
            .is_none_or(|charge| (Num::ZERO..=Num::ONE).contains(&charge));
        Position::from(self.origin).within_bound() && charge
    }
}
