use std::ops::Range;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Commands, Local, Query, Res};
use bevy_ecs::world::{EntityRef, World};
use campfire_sim::{Position, SimSet, StateRegistry};

use crate::combat::dead::Dead;
use crate::stats::unit_state::UnitState;
use crate::stats::unit_stats::UnitStats;
use crate::units::script_view::{RowFill, View};
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::values::grid::Grid;
use crate::vision::seen_by::SeenBy;
use crate::vision::sight::Sight;
use crate::vision::vision_grid::VisionGrid;

pub(crate) mod seen_by;
pub(crate) mod sight;
pub(crate) mod vision_api;
pub(crate) mod vision_data;
pub(crate) mod vision_grid;

/// The `vision` capability, for now its grid fog of war: what each team sees, which decides what
/// its clients receive and what `find_visible`, `nearest_visible` and `unit.can_see` return.
#[derive(Debug)]
pub struct Vision;

impl Vision {
    /// Adds vision to a match, on combat: in Vision, the last stage of a tick, each living unit
    /// with a sight reveals the grid cells around it to its team, and each unit learns the teams
    /// that see it. A match sees nothing until its mode gives the grid.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        schedule.add_systems(see.in_set(SimSet::Vision));
        registry.register_component::<SeenBy>();
        registry.register_component::<Sight>();
    }

    /// Gives the match the map's `grid`, and the number of its teams, the neutral one included.
    pub fn load_grid(world: &mut World, grid: Grid, teams: usize) {
        assert!(teams <= Team::LIMIT, "the mode's check limits the teams");
        world.insert_resource(VisionGrid { grid, teams });
    }

    /// The teams that see `unit`: those the last Vision stage found, or, before it ran, the
    /// unit's own; every team for an entity with no team, as a match without vision sees.
    fn seen_by(unit: &EntityRef<'_>) -> TeamSet {
        match (unit.get::<SeenBy>(), unit.get::<Team>()) {
            (Some(seen), _) => seen.get(),
            (None, Some(&team)) => TeamSet::of(team),
            (None, None) => TeamSet::ALL,
        }
    }
}

/// Fills a row of the script view with the teams that see the unit.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.seen_by = Vision::seen_by(unit);
}

/// Reveals the cells each living unit with a sight sees to its team, and those each such unit in
/// the `true_sight` state sees to its team's true sight, then gives each unit the teams that see
/// it: its own, and each whose cells hold it, or, for a stealthed unit, whose true sight does.
fn see(
    grid: Option<Res<'_, VisionGrid>>,
    seers: Query<'_, '_, (&Position, &Team, &Sight, Option<&UnitStats>), Without<Dead>>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &Position,
            &Team,
            Option<&UnitStats>,
            Option<&mut SeenBy>,
        ),
    >,
    mut commands: Commands<'_, '_>,
    mut revealed: Local<'_, Vec<u64>>,
    mut true_sight: Local<'_, Vec<u64>>,
) {
    let Some(grid) = grid else {
        return;
    };
    let words = grid.grid.cells().div_ceil(64);
    revealed.clear();
    revealed.resize(words * grid.teams, 0);
    true_sight.clear();
    true_sight.resize(words * grid.teams, 0);
    for (&pos, &team, sight, stats) in &seers {
        let run = usize::from(team.index()) * words;
        let truly = UnitStats::states_of(stats).contains(UnitState::TrueSight);
        grid.grid.spans_within(pos, sight.range(), |cells| {
            set_bits(&mut revealed[run..run + words], cells.clone());
            if truly {
                set_bits(&mut true_sight[run..run + words], cells);
            }
        });
    }
    for (entity, &pos, &team, stats, seen) in &mut units {
        let mut teams = TeamSet::of(team);
        let cell = grid
            .grid
            .cell_of(pos)
            .expect("every unit stands within the bounds, which the grid covers");
        let stealthed = UnitStats::states_of(stats).contains(UnitState::Stealthed);
        let sight = if stealthed { &true_sight } else { &revealed };
        for index in 0..grid.teams {
            if sight[index * words + cell / 64] & 1 << (cell % 64) != 0 {
                let index = u8::try_from(index).expect("teams fit u8");
                teams = teams.with(Team::new(index));
            }
        }
        match seen {
            Some(mut seen) if seen.get() != teams => *seen = SeenBy::new(teams),
            Some(_) => {}
            None => {
                commands.entity(entity).insert(SeenBy::new(teams));
            }
        }
    }
}

/// Sets the bits of `cells`, a run that is not empty, in `words`.
fn set_bits(words: &mut [u64], cells: Range<usize>) {
    debug_assert!(!cells.is_empty());
    let (first, last) = (cells.start / 64, (cells.end - 1) / 64);
    let from = u64::MAX << (cells.start % 64);
    let to = u64::MAX >> (63 - (cells.end - 1) % 64);
    if first == last {
        words[first] |= from & to;
    } else {
        words[first] |= from;
        words[first + 1..last].fill(u64::MAX);
        words[last] |= to;
    }
}

#[cfg(test)]
mod tests;
