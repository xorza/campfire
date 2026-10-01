use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, Position, StableId};

use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::living_unit::LivingUnit;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;

/// The units an attack may target: living units with health whose tags let them be targets.
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
            Option<&'static UnitTags>,
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
        let (&id, &pos, &team, body, tags) = self.units.get(self.index.get(target)?).ok()?;
        if UnitTags::effects_of(tags).blocks(Block::Target) {
            return None;
        }
        Some(LivingUnit {
            id,
            pos,
            team,
            radius: Body::radius_of(body),
            tags: tags.map_or(TagSet::default(), |tags| tags.tags),
        })
    }
}
