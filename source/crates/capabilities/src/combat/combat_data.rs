use serde::Deserialize;

use crate::combat::on_death::OnDeath;
use crate::values::scalar::Scalar;

/// A unit type's `combat` section. Its health and attack damage and speed are stats.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatData {
    pub attack: Option<AttackData>,
    #[serde(default)]
    pub on_death: OnDeath,
}

/// An attack as data declares it: its range in meters, its windup in milliseconds, and the speed
/// of its projectile in meters a second; no speed means melee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackData {
    pub range: Scalar,
    pub windup_ms: u64,
    pub projectile_speed: Option<Scalar>,
}
