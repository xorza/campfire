use bevy_ecs::change_detection::{DetectChanges, DetectChangesMut};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_core::{Body, Dead, OnMatch, Relations, Team, UnitTags};
use campfire_geometry::Shape;
use campfire_sim::{Position, SimTick};

use crate::fog::Fog;
use crate::reveals::Reveals;
use crate::seen_by::SeenBy;
use crate::sight::Sight;
use crate::vision_book::VisionBook;

/// What each vision group sees, and the teams that see each unit.
#[derive(Debug)]
pub struct Seeing;

impl Seeing {
    /// Reveals the cells each living unit with a sight sees to its vision group, but the cells of
    /// every brush other than the one it stands in, and those each such unit whose tags detect sees
    /// to its group's detection, and each reveal under way its cells, brush included, to its team's
    /// group, then gives each unit the teams that see it: its own group's, and those of each group
    /// whose cells hold it, or, for a unit its tags hide, whose detection does. The groups follow
    /// the relations as they change.
    pub(super) fn see(
        (vision, relations, tick): (
            Res<'_, VisionBook>,
            OnMatch<'_, '_, Relations>,
            Res<'_, SimTick>,
        ),
        mut reveals: ResMut<'_, Reveals>,
        seers: Query<'_, '_, (Entity, &Position, &Team, &Sight, Option<&UnitTags>), Without<Dead>>,
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &Position,
                &Team,
                Option<&Body>,
                Option<&UnitTags>,
                &mut SeenBy,
            ),
        >,
        mut fog: Local<'_, Fog>,
    ) {
        let Some(grid) = vision.grid() else {
            return;
        };
        let relations = relations.get();
        if relations.is_changed() || vision.is_changed() {
            fog.rebuild(grid, &relations);
        }
        fog.begin_tick();
        for (entity, &pos, &team, sight, tags) in &seers {
            let detects = UnitTags::properties_of(tags).detects();
            let slot = entity.index_u32() as usize;
            fog.sight(grid, slot, pos, team, sight.range(), detects);
        }
        // A write marks the reveals changed, so a tick with none writes nothing.
        if !reveals.is_empty() {
            reveals.run(tick.start(), |reveal| {
                fog.reveal(grid, reveal.pos, reveal.team, reveal.radius);
            });
        }
        for (entity, &pos, &team, body, tags, mut seen) in &mut units {
            let hidden = UnitTags::properties_of(tags).hidden();
            let boxed = match Body::shape_of(body) {
                Shape::Box(body) => Some(body),
                Shape::Circle(_) => None,
            };
            let slot = entity.index_u32() as usize;
            let teams = fog.seen_by(grid, slot, pos, boxed, team, hidden);
            seen.set_if_neq(SeenBy::new(teams));
        }
    }
}
