use bevy_ecs::change_detection::{DetectChanges, DetectChangesMut};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, With, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_common::Tick;
use campfire_geometry::Shape;
use campfire_players::{OnMatch, Player};
use campfire_sim::{Position, SimTick};
use campfire_units::{Body, Dead, MoveStep, SeenBy, Sighted};
use campfire_values::{Relation, Relations, Team, TeamSet, UnitTags};

use crate::explored_cells::ExploredCells;
use crate::fog::{Fog, Seer};
use crate::reveals::Reveals;
use crate::seen_cells::SeenCells;
use crate::sight::Sight;
use crate::vision_book::VisionBook;
use crate::vision_grid::VisionGrid;

/// What each vision group sees, and the teams that see each unit.
#[derive(Debug)]
pub struct Seeing;

/// The living units with a sight, which reveal the cells round them, and the riders that see
/// from their containers (transports.md, A garrison's riders see from it).
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
    (Without<Dead>, Sighted),
>;

impl Seeing {
    /// Reveals the cells each living unit with a sight sees to its vision group, but the cells of
    /// every brush other than the one it stands in, and those each such unit whose tags detect sees
    /// to its group's detection, and each reveal under way its cells, brush included, to its team's
    /// group, then gives each unit the teams that see it: its own group's, and those of each group
    /// whose cells hold it, or, for a unit its tags hide, whose detection does; and, on a grid that
    /// says so, a unit that does not walk, of a team another regards as neutral, that team too
    /// where it explored its cell, as Zero Hour draws a neutral immobile object in the fog
    /// (04-capabilities/vision.md, A neutral building stands in the fog). The groups follow the
    /// relations as they change.
    pub(super) fn see(
        (vision, relations, tick): (
            Res<'_, VisionBook>,
            OnMatch<'_, '_, Relations>,
            Res<'_, SimTick>,
        ),
        mut reveals: ResMut<'_, Reveals>,
        seers: Seers<'_, '_>,
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &Position,
                &Team,
                (Option<&Body>, Option<&UnitTags>, Has<MoveStep>),
                &mut SeenBy,
            ),
        >,
        (mut fog, explored): (
            ResMut<'_, Fog>,
            Query<'_, '_, (&Team, &ExploredCells), With<Player>>,
        ),
        mut neutral: Local<'_, Vec<TeamSet>>,
    ) {
        let grid = vision.grid().expect("a map with a grid is seen");
        let relations = relations.get();
        let changed = relations.is_changed() || vision.is_changed();
        let now = tick.start();
        Self::fill(&mut fog, grid, &relations, changed, &seers, |fog| {
            // A write marks the reveals changed, so a tick in which none ends writes nothing.
            if reveals.any_ended(now) {
                fog.expire(&mut reveals, now);
            }
            fog.count_new(grid, &reveals);
        });
        let teams_of = |at: usize| Team::new(u8::try_from(at).expect("a grid's teams fit u8"));
        neutral.clear();
        neutral.extend((0..grid.teams).map(|of| {
            let viewers = (0..grid.teams).map(teams_of);
            let neutral = viewers
                .filter(|&viewer| relations.between(viewer, teams_of(of)) == Relation::Neutral);
            neutral.fold(TeamSet::NONE, TeamSet::with)
        }));
        for (entity, &position, &team, (body, tags, walks), mut seen) in &mut units {
            let hidden = tags.is_some_and(|tags| tags.properties.is_hidden());
            let boxed = match Body::shape_of(body) {
                Shape::Box(body) => Some(body),
                Shape::Circle(_) => None,
            };
            let slot = entity.index_u32() as usize;
            let mut teams = fog.seen_by(grid, slot, position.get(), boxed, team, hidden);
            if grid.neutral_in_fog && !walks && !hidden {
                let cell = grid.grid.cell_of(position.get());
                let explored = |viewer: &Team| {
                    !grid.shroud
                        || explored.iter().any(|(&of, cells)| {
                            of == *viewer && cell.is_some_and(|cell| cells.cells().holds(cell))
                        })
                };
                let viewers = neutral[usize::from(team.get())].iter().filter(explored);
                teams = viewers.fold(teams, TeamSet::with);
            }
            seen.set_if_neq(SeenBy::new(teams));
        }
    }

    /// Writes each player's cells, after the stage's sight: those its team's group sees, and, of
    /// a shrouded map, those it explored, each only where a cell changed, so its client receives
    /// each change once (04-capabilities/vision.md, The server sends a team's cells to its player).
    pub(super) fn share(
        (vision, fog): (Res<'_, VisionBook>, Res<'_, Fog>),
        mut players: Query<'_, '_, (&Team, &mut SeenCells, &mut ExploredCells), With<Player>>,
    ) {
        let grid = vision.grid().expect("a map with a grid is seen");
        for (&team, mut seen, mut explored) in &mut players {
            let words = fog.seen_words(team);
            if seen.bypass_change_detection().0.set(words) {
                seen.set_changed();
            }
            if grid.shroud && explored.bypass_change_detection().0.add(words) {
                explored.set_changed();
            }
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
        // The last stage left the reveals under way in it, or made since.
        debug_assert!(
            tick.start().get() == 0 || !reveals.any_ended(Tick::new(tick.start().get() - 1)),
            "the reveals the last Vision stage kept"
        );
        Self::fill(&mut fog, grid, &relations, true, &seers, |fog| {
            fog.count_new(grid, &reveals);
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
}
