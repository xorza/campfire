use bevy_ecs::world::EntityWorldMut;

use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;

/// A unit type's combat values: what a new unit of that type starts with. A unit with no attack,
/// such as a structure that only stands, takes damage and dies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combatant {
    pub health: Health,
    pub attack: Option<AttackStats>,
    pub on_death: OnDeath,
}

impl Combatant {
    /// Gives `unit` the combat components of a new unit of this type.
    pub fn insert(self, unit: &mut EntityWorldMut<'_>) {
        unit.insert((self.health, self.on_death, RecentAttackers::default()));
        if let Some(attack) = self.attack {
            unit.insert((attack, AttackState::default()));
        }
    }
}

#[cfg(test)]
mod internals {
    use bevy_ecs::bundle::Bundle;

    use super::*;
    use crate::units::team::Team;

    impl Combatant {
        /// The components of a new unit of this type on `team`, which has an attack: its team,
        /// which the spawn gives, and those `insert` gives.
        pub(crate) fn bundle(self, team: Team) -> impl Bundle {
            (
                team,
                self.health,
                self.attack.expect("a test unit has an attack"),
                AttackState::default(),
                self.on_death,
                RecentAttackers::default(),
            )
        }
    }
}
