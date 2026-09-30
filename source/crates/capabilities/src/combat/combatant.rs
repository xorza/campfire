use bevy_ecs::bundle::Bundle;

use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::combat::team::Team;

/// A unit type's combat section: what a new unit of that type starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combatant {
    pub health: Health,
    pub attack: AttackStats,
    pub on_death: OnDeath,
}

impl Combatant {
    /// The components of a new unit of this type on `team`, which the spawn gives.
    pub fn bundle(self, team: Team) -> impl Bundle {
        (
            team,
            self.health,
            self.attack,
            AttackState::default(),
            self.on_death,
        )
    }
}
