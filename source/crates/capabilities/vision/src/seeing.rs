use bevy_ecs::change_detection::{DetectChanges, DetectChangesMut};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_common::Tick;
use campfire_geometry::Shape;
use campfire_players::OnMatch;
use campfire_sim::{Position, SimTick};
use campfire_units::{Body, Dead, SeenBy};
use campfire_values::{Relations, Team, UnitTags};

use crate::fog::{Fog, Seer};
use crate::reveals::{Reveal, Reveals};
use crate::sight::Sight;
use crate::vision_book::VisionBook;
use crate::vision_grid::VisionGrid;

/// What each vision group sees, and the teams that see each unit.
#[derive(Debug)]
pub struct Seeing;

/// The living units with a sight, which reveal the cells round them.
type Seers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Position,
        &'static Team,
        &'static Sight,
        Option<&'static UnitTags>,
    ),
    Without<Dead>,
>;

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
        reveals: ResMut<'_, Reveals>,
        seers: Seers<'_, '_>,
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
        mut fog: ResMut<'_, Fog>,
    ) {
        let grid = vision.grid().expect("a map with a grid is seen");
        let relations = relations.get();
        let changed = relations.is_changed() || vision.is_changed();
        // A write marks the reveals changed, so a tick with none writes nothing.
        let reveals = (!reveals.is_empty()).then(|| reveals.into_inner());
        let now = tick.start();
        Self::fill(&mut fog, grid, &relations, changed, &seers, |fog| {
            if let Some(reveals) = reveals {
                reveals.run(now, |reveal| Self::reveal(fog, grid, reveal));
            }
        });
        for (entity, &position, &team, body, tags, mut seen) in &mut units {
            let hidden = tags.is_some_and(|tags| tags.properties.is_hidden());
            let boxed = match Body::shape_of(body) {
                Shape::Box(body) => Some(body),
                Shape::Circle(_) => None,
            };
            let slot = entity.index_u32() as usize;
            let teams = fog.seen_by(grid, slot, position.get(), boxed, team, hidden);
            seen.set_if_neq(SeenBy::new(teams));
        }
    }

    /// Fills a fog that holds no Vision stage's sight, as a new or a restored match's, as the
    /// tick starts: with what the last Vision stage saw, from the units and the reveals as that
    /// stage left them, so a placement in this tick reads the sight the original match's did.
    pub(super) fn refill(
        (vision, relations, tick): (
            Res<'_, VisionBook>,
            OnMatch<'_, '_, Relations>,
            Res<'_, SimTick>,
        ),
        reveals: Res<'_, Reveals>,
        seers: Seers<'_, '_>,
        mut fog: ResMut<'_, Fog>,
    ) {
        let grid = vision.grid().expect("a map with a grid is seen");
        let relations = relations.get();
        let seen = tick.start().get().checked_sub(1).map(Tick::new);
        Self::fill(&mut fog, grid, &relations, true, &seers, |fog| {
            if let Some(seen) = seen {
                reveals.each_of(seen, |reveal| Self::reveal(fog, grid, reveal));
            }
        });
    }

    /// Whether the fog holds no Vision stage's sight, as a new or a restored match's does.
    pub(super) fn is_unfilled(fog: Res<'_, Fog>) -> bool {
        !fog.is_filled()
    }

    /// Fills `fog` with what `seers` see on `grid`, its groups built again from `relations`
    /// first when they `changed`, then with what `reveals` adds.
    fn fill(
        fog: &mut Fog,
        grid: &VisionGrid,
        relations: &Relations,
        changed: bool,
        seers: &Seers<'_, '_>,
        reveals: impl FnOnce(&mut Fog),
    ) {
        if changed || !fog.is_filled() {
            fog.rebuild(grid, relations);
        }
        let seers = seers
            .iter()
            .map(|(entity, position, &team, sight, tags)| Seer {
                slot: entity.index_u32() as usize,
                position: position.get(),
                team,
                range: sight.range(),
                detects: tags.is_some_and(|tags| tags.properties.detects()),
            });
        fog.see(grid, seers);
        reveals(fog);
    }

    fn reveal(fog: &mut Fog, grid: &VisionGrid, reveal: &Reveal) {
        fog.reveal(grid, reveal.position, reveal.team, reveal.radius);
    }
}
