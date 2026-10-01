use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, Position, StableId};

use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::stats::unit_stats::UnitStats;
use crate::units::body::Body;
use crate::units::living_unit::LivingUnit;
use crate::units::team::Team;

/// The units an attack may target: living units with health whose states let them be targets.
/// Every capability that chooses or checks a target goes through it, so all agree on what a valid
/// target is.
#[derive(SystemParam, Debug)]
pub(crate) struct Targets<'w, 's> {
    index: Res<'w, EntityIndex>,
    units: Query<
        'w,
        's,
        (
            &'static StableId,
            &'static Position,
            &'static Team,
            Option<&'static Body>,
            Option<&'static UnitStats>,
        ),
        (With<Health>, Without<Dead>),
    >,
}

impl Targets<'_, '_> {
    /// `target`, when it is a living enemy of `team`.
    pub(crate) fn enemy(&self, team: Team, target: StableId) -> Option<LivingUnit> {
        self.living(target)
            .filter(|unit| team.is_enemy_of(unit.team))
    }

    /// `target`, when it is a living unit that may be a target.
    pub(crate) fn living(&self, target: StableId) -> Option<LivingUnit> {
        let (&id, &pos, &team, body, stats) = self.units.get(self.index.get(target)?).ok()?;
        if !UnitStats::states_of(stats).targetable() {
            return None;
        }
        let radius = Body::radius_of(body);
        Some(LivingUnit {
            id,
            pos,
            team,
            radius,
        })
    }
}
