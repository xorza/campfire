use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_sim::{Capability, SimSet};

use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::geometry::grid::Grid;
use crate::geometry::polygon::Polygon;
use crate::units::by_type::ByType;
use crate::vision::brush_map::BrushMap;
use crate::vision::reveals::Reveals;
use crate::vision::seeing::Seeing;
use crate::vision::seen_by::SeenBy;
use crate::vision::sight::Sight;
use crate::vision::vision_api::VisionApi;
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
pub(crate) mod team_sight;
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

impl Declares for Vision {
    const MODULE: Option<Capability> = Some(Capability::Vision);

    fn declare<D: Declarations>(d: &mut D) {
        d.resource::<Reveals>();
        d.component::<SeenBy>();
        d.component::<Sight>();
        d.book::<ByType<Sight>>();
        d.column::<VisionColumn, RowParts>(VisionColumn::fill_row);
        d.api(VisionApi::register);
    }
}

impl Vision {
    /// Adds vision to a match, on combat: in Vision, the last stage of a tick, each living unit
    /// with a sight reveals the grid cells around it to its vision group, and each unit learns the
    /// teams that see it, with the reveals under way. A match sees nothing until its mode gives
    /// the grid. The sights of the delivery types, which no kit holds, are a book of their own.
    /// What a team sees is the server's: a predicting client holds the units its team sees, as
    /// the server sends them, and reveals nothing.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        if !cx.predicting() {
            cx.schedule.add_systems(Seeing::see.in_set(SimSet::Vision));
        }
        cx.world.insert_resource(Reveals::default());
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
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_math::{Num, Vec3};
    use campfire_script::{Budget, ScriptHost};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, SimTick, StableId};

    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::scripts::ctx::Ctx;
    use crate::scripts::error::CallError;
    use crate::scripts::error::internals::FailureKind;
    use crate::stats::life::Life;
    use crate::units::Units;
    use crate::units::tag_properties::TagProperties;
    use crate::units::team::Team;
    use crate::units::team_set::TeamSet;
    use crate::units::unit_tags::UnitTags;
    use crate::units::units_world::UnitsWorldCrate;
    use crate::vision::Vision;
    use crate::vision::brush_map::BrushMap;
    use crate::vision::seen_by::SeenBy;
    use crate::vision::sight::Sight;
    use crate::vision::vision_column::VisionColumn;
    use crate::vision::vision_grid::VisionGrid;

    impl Vision {
        /// Gives the match a grid of one cell, for `teams` teams, that no Vision stage reads: the
        /// `SeenBy` a test sets on its units alone tells what each team sees.
        pub(crate) fn fog_grid(world: &mut World, teams: usize) {
            let bounds = Bounds::new([-Num::HALF; 2], [Num::HALF; 2]).unwrap();
            let grid = Grid::new(Num::ONE, bounds).unwrap();
            let brush = BrushMap::new(&grid, &[]);
            world.insert_resource(VisionGrid { grid, brush, teams });
        }

        /// A unit of `team` at `x`, `z` with the life pool, so it may be a target, and seeing
        /// `sight` when given.
        pub(crate) fn spawn_seer(
            world: &mut World,
            x: i64,
            z: i64,
            team: Team,
            sight: Option<i64>,
        ) -> StableId {
            let life = Life::full(Num::int(100));
            let id = Units::spawn(
                world,
                Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap(),
                team,
                life,
            );
            if let Some(range) = sight {
                world.insert_parts(id, Sight::new(Num::int(range)).unwrap());
            }
            id
        }

        /// Gives unit `id` tags with `properties`, as its modifiers would.
        pub(crate) fn set_properties(world: &mut World, id: StableId, properties: TagProperties) {
            world.insert_parts(id, UnitTags::with_properties(properties));
        }

        /// Runs `ctx.reveal(at.pos, radius, ms)` as `actor` thinks, or as the mode with no actor,
        /// then applies what it queued; why it failed, when it did.
        pub(crate) fn reveal(
            world: &mut World,
            actor: Option<StableId>,
            at: StableId,
            radius: &str,
            ms: i64,
        ) -> Result<(), FailureKind> {
            let ctx = world.non_send::<Ctx>().clone();
            ctx.view().read(world);
            let now = world.resource::<SimTick>().start();
            match actor {
                Some(actor) => ctx.frame().begin_think(world, actor, now),
                None => ctx.frame().begin_mode(world, false, now),
            }
            let at = ctx.view().unit(at).unwrap();
            let source = format!("fn probe(ctx, at) {{ ctx.reveal(at.pos, {radius}, {ms}) }}");
            let returned = {
                let mut host = world.non_send_mut::<ScriptHost>();
                let script = host.compile(&source).unwrap();
                let mut budget = Budget::new(u64::MAX);
                host.call(&mut budget, script, "probe", (ctx.clone(), at))
            };
            let returned = returned.map_err(|error| CallError::from_script(error).kind())?;
            assert!(returned.is_unit());
            ctx.apply(world);
            Ok(())
        }

        pub(crate) fn seen_by(world: &World, id: StableId) -> TeamSet {
            let relations = world.relations();
            let parts = (world.try_part::<SeenBy>(id), world.try_part::<Team>(id));
            VisionColumn::seen_by(parts, relations)
        }
    }
}

#[cfg(test)]
mod tests;
