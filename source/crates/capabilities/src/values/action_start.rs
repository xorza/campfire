use campfire_math::Num;
use campfire_sim::Position;
use serde::{Deserialize, Serialize};

/// How an action started: where its unit stood, and, for a charged action, the share of its most
/// it charged, from 0 to 1. Every hook of the action reads them, its delivery's included, as
/// `ctx.origin` and `ctx.charge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActionStart {
    pub(crate) origin: Position,
    pub(crate) charge: Option<Num>,
}
