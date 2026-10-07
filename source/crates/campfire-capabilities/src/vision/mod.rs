use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::ROQueryItem;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_sim::{Position, SimSet, SimTick, StateRegistry};

use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::relations::Relations;
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::unit_tags::UnitTags;
use crate::values::grid::Grid;
use crate::values::polygon::Polygon;
use crate::vision::brush_map::BrushMap;
use crate::vision::fog::Fog;
use crate::vision::reveals::Reveals;
use crate::vision::seen_by::SeenBy;
use crate::vision::sight::Sight;
use crate::vision::vision_column::VisionColumn;
use crate::vision::vision_grid::VisionGrid;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod brush_map;
pub(crate) mod fog;
pub(crate) mod reveals;
pub(crate) mod seen_by;
pub(crate) mod sight;
pub(crate) mod sight_cache;
pub(crate) mod sight_maps;
pub(crate) mod vision_api;
pub(crate) mod vision_column;
pub(crate) mod vision_data;
pub(crate) mod vision_effect;
pub(crate) mod vision_grid;
pub(crate) mod vision_groups;

/// The `vision` capability, for now its grid fog of war: what each vision group sees, which
/// decides what its teams' clients receive and what `find_visible`, `nearest_visible` and
/// `unit.can_see` return.
#[derive(Debug)]
pub struct Vision;

impl Vision {
    /// Adds vision to a match, on combat: in Vision, the last stage of a tick, each living unit
    /// with a sight reveals the grid cells around it to its vision group, and each unit learns the
    /// teams that see it, with the reveals under way. A match sees nothing until its mode gives
    /// the grid. The sights of the delivery types, which no kit holds, are a book of their own.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(VisionColumn::default());
        view.add_source::<RowParts, _>(world, fill_row);
        schedule.add_systems(see.in_set(SimSet::Vision));
        world.insert_resource(ByType::<Sight>::default());
        world.insert_resource(Reveals::default());
        registry.register_resource::<Reveals>();
        registry.register_component::<SeenBy>();
        registry.register_component::<Sight>();
    }

    /// Gives the match the map's `grid`, with the brush of `brush`'s areas, in the map's order,
    /// and the number of its teams.
    pub fn load_grid(world: &mut World, grid: Grid, brush: &[Polygon], teams: usize) {
        assert!(
            teams <= VisionGrid::MAX_TEAMS,
            "the mode's check limits the teams of a map with vision"
        );
        let brush = BrushMap::new(&grid, brush);
        world.insert_resource(VisionGrid { grid, brush, teams });
    }

    /// The teams that see `unit`: those the last Vision stage found, or, before it ran, the
    /// unit's vision group under `relations`, as that stage would give it at the least; every
    /// team for an entity with no team, as a match without vision sees.
    fn seen_by(parts: ROQueryItem<'_, '_, RowParts>, relations: &Relations) -> TeamSet {
        match parts {
            (Some(seen), _) => seen.get(),
            (None, Some(&team)) => relations.vision_group(team),
            (None, None) => TeamSet::ALL,
        }
    }
}

/// The parts of a unit vision reads into its row: the teams that saw it, and its team.
type RowParts = (Option<&'static SeenBy>, Option<&'static Team>);

/// Fills a row of the script view with the teams that see the unit.
fn fill_row(parts: ROQueryItem<'_, '_, RowParts>, fill: &mut RowFill<'_, VisionColumn>) {
    let seen_by = Vision::seen_by(parts, fill.relations);
    fill.column.push(seen_by);
}

/// Reveals the cells each living unit with a sight sees to its vision group, but the cells of every
/// brush other than the one it stands in, and those each such unit whose tags detect sees to its
/// group's detection, and each reveal under way its cells, brush included, to its team's group,
/// then gives each unit the teams that see it: its own group's, and those of each group whose cells
/// hold it, or, for a unit its tags hide, whose detection does. The groups follow the relations as
/// they change.
fn see(
    (grid, relations, tick): (
        Option<Res<'_, VisionGrid>>,
        Res<'_, Relations>,
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
            Option<&UnitTags>,
            Option<&mut SeenBy>,
        ),
    >,
    mut commands: Commands<'_, '_>,
    mut fog: Local<'_, Fog>,
) {
    let Some(grid) = grid else {
        return;
    };
    if relations.is_changed() || grid.is_changed() {
        fog.rebuild(&grid, &relations);
    }
    fog.begin_tick();
    for (entity, &pos, &team, sight, tags) in &seers {
        let detects = UnitTags::properties_of(tags).detects();
        let slot = entity.index_u32() as usize;
        fog.sight(&grid, slot, pos, team, sight.range(), detects);
    }
    reveals.run(tick.start(), |reveal| {
        fog.reveal(&grid, reveal.pos, reveal.team, reveal.radius);
    });
    for (entity, &pos, &team, tags, seen) in &mut units {
        let hidden = UnitTags::properties_of(tags).hidden();
        let teams = fog.seen_by(&grid, pos, team, hidden);
        match seen {
            Some(mut seen) if seen.get() != teams => *seen = SeenBy::new(teams),
            Some(_) => {}
            None => {
                commands.entity(entity).insert(SeenBy::new(teams));
            }
        }
    }
}

#[cfg(test)]
mod tests;
