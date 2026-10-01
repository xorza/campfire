use bevy_ecs::world::EntityWorldMut;

use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;

/// A unit type's combat values: what a new unit of that type starts with, beside its pools. A
/// unit with no attack, such as a structure that only stands, takes damage and dies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combatant {
    pub attack: Option<AttackStats>,
    pub on_death: OnDeath,
}

impl Combatant {
    /// Gives `unit` the combat components of a new unit of this type.
    pub fn insert(self, unit: &mut EntityWorldMut<'_>) {
        unit.insert((self.on_death, RecentAttackers::default()));
        if let Some(attack) = self.attack {
            unit.insert((attack, AttackState::default()));
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use bevy_ecs::bundle::Bundle;
    use campfire_math::Num;

    use crate::combat::attack_state::AttackState;
    use crate::combat::combatant::Combatant;
    use crate::combat::on_death::OnDeath;
    use crate::combat::recent_attackers::RecentAttackers;
    use crate::stats::pools::Pools;
    use crate::units::team::Team;

    /// A test unit's combat values, which have an attack, and the life pool it starts with.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Armed {
        pub(crate) combatant: Combatant,
        pub(crate) life: Num,
    }

    impl Armed {
        /// The same unit, which `on_death` says whether it stays when it dies.
        pub(crate) const fn on_death(self, on_death: OnDeath) -> Armed {
            Armed {
                combatant: Combatant {
                    on_death,
                    ..self.combatant
                },
                life: self.life,
            }
        }

        /// The components of a new unit of this type on `team`: its team, which the spawn
        /// gives, its life pool, which its kit gives, and those `Combatant::insert` gives.
        pub(crate) fn bundle(self, team: Team) -> impl Bundle {
            (
                team,
                Pools::life(self.life),
                self.combatant.attack.expect("a test unit has an attack"),
                AttackState::default(),
                self.combatant.on_death,
                RecentAttackers::default(),
            )
        }
    }
}
