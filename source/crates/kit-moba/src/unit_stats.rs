use crate::attack_stats::AttackStats;
use crate::health::Health;
use crate::move_step::MoveStep;

/// What a new unit starts with. A unit with no move step never moves: a tower.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitStats {
    pub health: Health,
    pub attack: AttackStats,
    pub step: Option<MoveStep>,
}
