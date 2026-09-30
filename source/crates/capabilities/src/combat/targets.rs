use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, Position, StableId};

use crate::combat::attack_stats::{AttackStats, ground_offset};
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::living_unit::LivingUnit;
use crate::combat::team::Team;

/// The units an attack may target: living units with health. Every capability that chooses or
/// checks a target goes through it, so all agree on what a valid target is.
#[derive(SystemParam, Debug)]
pub(crate) struct Targets<'w, 's> {
    index: Res<'w, EntityIndex>,
    units: Query<
        'w,
        's,
        (&'static StableId, &'static Position, &'static Team),
        (With<Health>, Without<Dead>),
    >,
}

impl Targets<'_, '_> {
    /// Where `target` is, when it is a living enemy of `team`.
    pub(crate) fn enemy_at(&self, team: Team, target: StableId) -> Option<Position> {
        self.living(target)
            .filter(|unit| team.is_enemy_of(unit.team))
            .map(|unit| unit.pos)
    }

    /// `target`, when it is a living unit.
    pub(crate) fn living(&self, target: StableId) -> Option<LivingUnit> {
        let (&id, &pos, &team) = self.units.get(self.index.get(target)?).ok()?;
        Some(LivingUnit { id, pos, team })
    }

    /// The nearest living enemy of `team` that `stats` reach from `from`, by exact distance on the
    /// ground plane, the lower stable id on a tie.
    pub(crate) fn nearest_enemy(
        &self,
        team: Team,
        from: Position,
        stats: &AttackStats,
    ) -> Option<StableId> {
        self.units
            .iter()
            .filter(|&(_, &at, &theirs)| team.is_enemy_of(theirs) && stats.reaches(from, at))
            .min_by_key(|&(&id, &at, _)| (ground_offset(from, at).length_squared_bits(), id))
            .map(|(&id, _, _)| id)
    }
}
