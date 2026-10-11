use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::world::World;
use campfire_sim::{SimSet, StateRegistry};

use crate::geometry::grid::Grid;
use crate::geometry::polygon::Polygon;
use crate::state_types::StateTypes;
use crate::units::by_type::ByType;
use crate::units::view::View;
use crate::vision::brush_map::BrushMap;
use crate::vision::reveals::Reveals;
use crate::vision::seeing::Seeing;
use crate::vision::seen_by::SeenBy;
use crate::vision::sight::Sight;
use crate::vision::vision_column::{RowParts, VisionColumn};
use crate::vision::vision_grid::VisionGrid;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod brush_map;
pub(crate) mod fog;
pub(crate) mod reveals;
pub(crate) mod seeing;
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
    /// Lists the state types it adds (design 14, D9).
    pub(crate) fn state_types<T: StateTypes>(types: &mut T) {
        types.resource::<Reveals>();
        types.component::<SeenBy>();
        types.component::<Sight>();
    }

    /// Adds vision to a match, on combat: in Vision, the last stage of a tick, each living unit
    /// with a sight reveals the grid cells around it to its vision group, and each unit learns the
    /// teams that see it, with the reveals under way. A match sees nothing until its mode gives
    /// the grid. The sights of the delivery types, which no kit holds, are a book of their own.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(VisionColumn::default());
        view.add_source::<RowParts, _>(world, VisionColumn::fill_row);
        schedule.add_systems(Seeing::see.in_set(SimSet::Vision));
        world.insert_resource(ByType::<Sight>::default());
        world.insert_resource(Reveals::default());
        Self::state_types(registry);
    }

    /// Gives the match the map's `grid`, with the brush of `brush`'s areas, in the map's order,
    /// and the number of its teams.
    pub fn load_grid(world: &mut World, grid: Grid, brush: &[Polygon], teams: usize) {
        debug_assert!(
            world.contains_resource::<Reveals>(),
            "a map's vision grid is vision's, which the load checked the mode declares"
        );
        assert!(
            teams <= VisionGrid::MAX_TEAMS,
            "the mode's check limits the teams of a map with vision"
        );
        let brush = BrushMap::new(&grid, brush);
        world.insert_resource(VisionGrid { grid, brush, teams });
    }
}

#[cfg(test)]
mod tests;
