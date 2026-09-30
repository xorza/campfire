use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, Position, StableId};

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
}
