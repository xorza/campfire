use campfire_common::Tick;
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// An attack's pre-attack, a phase apart from its windup (04-capabilities/combat.md, A
/// pre-attack): the weapon of `slot` aimed at `target` until `done_at`, which a start of the
/// weapon may not come before, and whether a start spent it, so a pre-attack of each target or
/// clip holds no other until its target or its clip changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreAttack {
    pub slot: u8,
    pub target: StableId,
    pub done_at: Tick,
    pub spent: bool,
}

impl PreAttack {
    /// Whether it is the pre-attack of the weapon of `slot` at `target`.
    pub fn of(&self, slot: u8, target: StableId) -> bool {
        self.slot == slot && self.target == target
    }
}
