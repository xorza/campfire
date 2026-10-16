#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_sim::Capability;
use campfire_units::UnitEntity;

use crate::vision_column::RowReads;

#[cfg(feature = "internals")]
pub mod bench;
mod brush_map;
mod fog;
mod reveals;
mod seeing;
mod seen_by;
mod seen_targets;
mod sight;
mod sight_cache;
mod sight_maps;
mod team_sight;
mod vision_api;
mod vision_book;
mod vision_column;
mod vision_data;
mod vision_effect;
mod vision_grid;
mod vision_groups;

pub use crate::brush_map::BrushMap;
pub use crate::fog::Fog;
pub use crate::reveals::{Reveal, Reveals};
pub use crate::seeing::Seeing;
pub use crate::seen_by::SeenBy;
pub use crate::seen_targets::SeenTargets;
pub use crate::sight::Sight;
pub use crate::team_sight::{ByHolding, ByVision, TeamSight};
pub use crate::vision_api::VisionApi;
pub use crate::vision_book::VisionBook;
pub use crate::vision_column::VisionColumn;
pub use crate::vision_data::VisionData;
pub use crate::vision_grid::VisionGrid;

/// The `vision` capability, for now its grid fog of war: what each vision group sees, which
/// decides what its teams' clients receive and what `find_visible`, `nearest_visible` and
/// `unit.can_see` return.
#[derive(Debug)]
pub struct Vision;

campfire_common::enum_all! {
    /// The steps of `vision`.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum VisionSet {
        /// Each team marks what it may see.
        See,
    }
    /// Every step.
    pub const ALL;
}

impl ModuleSet for VisionSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Vision);
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            Self::See => Phase::End,
        })
    }
}

impl Module for Vision {
    const CAPABILITY: Option<Capability> = Some(Capability::Vision);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<VisionSet>();
        d.resource::<Reveals>();
        d.component::<SeenBy>();
        d.component::<Sight>();
        d.book::<ByType<Sight>>();
        d.book::<VisionBook>();
        d.column::<VisionColumn, RowReads>(VisionColumn::fill_row);
        d.api(VisionApi::register);
        d.part::<UnitEntity, Sight>(|at| at.of_type::<Sight>().copied());
        // What each team sees is the server's (14-replication-audiences.md, Prediction reads only
        // what its client holds): a predicting client holds none.
        d.server_part::<UnitEntity, SeenBy>();
    }

    /// Adds vision to a match, on combat: in Vision, the last stage of a tick, each living unit
    /// with a sight reveals the grid cells around it to its vision group, and each unit learns the
    /// teams that see it, with the reveals under way. A match sees nothing until its mode gives
    /// the grid. What a team sees is the server's: a predicting client holds the units its team sees, as
    /// the server sends them, and reveals nothing.
    fn install(cx: &mut InstallCx<'_>) {
        cx.server(
            Seeing::see
                .run_if(VisionBook::has_grid)
                .in_set(VisionSet::See),
        );
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_declare::Book;
    use campfire_geometry::{Bounds, Grid, Polygon};
    use campfire_math::Num;
    use campfire_script::internals::FailureKind;
    use campfire_script::{Budget, CallError, CallStart, Ctx, ScriptHost, ScriptRole};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{SimTick, StableId};
    use campfire_units::UnitsWorld;
    use campfire_values::{Team, TeamSet};

    use crate::Vision;
    use crate::brush_map::BrushMap;
    use crate::seen_by::SeenBy;
    use crate::vision_book::VisionBook;
    use crate::vision_column::VisionColumn;
    use crate::vision_grid::VisionGrid;

    impl Vision {
        /// Gives the match the map's `grid`, with the brush of `brush`'s areas, in the map's
        /// order, and the number of its teams, as the book builder gives a mode's.
        pub fn load_grid(world: &mut World, grid: Grid, brush: &[Polygon], teams: usize) {
            assert!(
                teams <= VisionGrid::MAX_TEAMS,
                "the mode's check limits the teams of a map with vision"
            );
            let brush = BrushMap::new(&grid, brush);
            (VisionBook(Some(VisionGrid { grid, brush, teams }))).load_into(world);
        }

        /// Takes the map's vision grid out of a match that holds one, so no Vision stage runs
        /// until a test loads a grid: a unit keeps the teams the last stage found, and one no
        /// stage spoke of is seen by its own vision group alone, or by every team when it has
        /// none.
        pub fn clear_grid(world: &mut World) {
            if world.contains_resource::<VisionBook>() {
                VisionBook::default().load_into(world);
            }
        }
    }

    /// The sights of the tests' seers, in meters: none, then each range one sees.
    pub const SIGHTS: [Option<i64>; 8] = [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(3),
        Some(4),
        Some(6),
        Some(20),
    ];

    impl Vision {
        /// Gives the match a grid of one cell, for `teams` teams, that no Vision stage reads: the
        /// `SeenBy` a test sets on its units alone tells what each team sees.
        pub fn fog_grid(world: &mut World, teams: usize) {
            let bounds = Bounds::new([-Num::HALF; 2], [Num::HALF; 2]).unwrap();
            let grid = Grid::new(Num::ONE, bounds).unwrap();
            let brush = BrushMap::new(&grid, &[]);
            // A match with no vision declares no vision book: the grid makes its orders read
            // the sight a test sets, as a match with vision does.
            world.insert_resource(VisionBook(Some(VisionGrid { grid, brush, teams })));
        }

        /// The name of the seer of `sight`, one of `SIGHTS`.
        pub fn seer(sight: Option<i64>) -> String {
            debug_assert!(SIGHTS.contains(&sight), "a sight of the seers");
            sight.map_or_else(|| "blind".to_owned(), |range| format!("seer of {range}"))
        }

        /// Runs `ctx.reveal(at.pos, radius, ms)` as `actor` thinks, or as the mode with no actor,
        /// then applies what it queued; why it failed, when it did.
        pub fn reveal(
            world: &mut World,
            actor: Option<StableId>,
            at: StableId,
            radius: &str,
            ms: i64,
        ) -> Result<(), FailureKind> {
            let ctx = world.non_send::<Ctx>().clone();
            ctx.view().read(world);
            let now = world.resource::<SimTick>().start();
            let start = match actor {
                Some(actor) => CallStart::think(actor, now),
                None => CallStart::mode(ScriptRole::Mode, now),
            };
            ctx.frame().begin(world, start).unwrap();
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

        pub fn seen_by(world: &World, id: StableId) -> TeamSet {
            let relations = world.relations();
            let parts = (world.try_part::<SeenBy>(id), world.try_part::<Team>(id));
            VisionColumn::seen_by(parts, relations)
        }
    }
}
