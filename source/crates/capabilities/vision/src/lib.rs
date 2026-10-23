// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_common::EnumAll;
use campfire_declarations::{ById, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_sim::Capability;
use campfire_units::{SeenBy, UnitEntity, UnitsSystems};

use crate::vision_column::RowReads;
use campfire_values::UnitType;

#[cfg(feature = "internals")]
pub mod bench;
mod brush_map;
mod error;
mod fog;
mod grid_data;
mod reveals;
mod seeing;
mod seen_targets;
mod sight;
mod sight_cache;
mod sight_maps;
mod team_sight;
mod vision_api;
mod vision_book;
mod vision_books;
mod vision_column;
mod vision_data;
mod vision_effect;
mod vision_grid;
mod vision_groups;

pub use crate::brush_map::BrushMap;
pub use crate::error::VisionDataError;
pub use crate::fog::Fog;
pub use crate::grid_data::{BrushData, GridData};
pub use crate::reveals::{Reveal, Reveals};
pub use crate::seeing::Seeing;
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

/// The steps of `vision`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum VisionSystems {
    /// A fog that holds no Vision stage's sight, a new or a restored match's, is filled.
    Refill,
    /// Each team marks what it may see.
    See,
}

impl ModuleSystems for VisionSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Refill => SetPlace::at(UnitsSystems::Begin),
            Self::See => SetPlace::at(UnitsSystems::End),
        }
    }
}

impl Module for Vision {
    const CAPABILITY: Option<Capability> = Some(Capability::Vision);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Systems = VisionSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<VisionData>();
        d.section::<GridData>();
        d.resource::<Reveals>();
        d.cache::<Fog>();
        d.component::<SeenBy>();
        d.component::<Sight>();
        d.book::<ById<UnitType, Sight>>(Sight::book);
        d.book::<VisionBook>(VisionBook::build);
        d.rule(VisionBook::grid_rule);
        d.column::<VisionColumn, RowReads>(VisionColumn::fill_row);
        d.api(VisionApi::register);
        d.part::<UnitEntity, Sight>(|at| at.of_type::<Sight>().copied());
        // What each team sees is the server's (10-replication-audiences.md, Prediction reads only
        // what its client holds): a predicting client holds none.
        d.server_part::<UnitEntity, SeenBy>();
    }

    /// Adds vision to a match, on combat, when its map has a grid: in Vision, the last stage of a
    /// tick, each living unit with a sight reveals the grid cells around it to its vision group,
    /// and each unit learns the teams that see it, with the reveals under way. A match of no grid
    /// sees nothing. What a team sees is the server's: a predicting client holds the units its
    /// team sees, as the server sends them, and reveals nothing.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        if !cx.book::<VisionBook>().is_some_and(VisionBook::has_grid) {
            return;
        }
        cx.server(
            VisionSystems::Refill,
            Seeing::refill.run_if(Seeing::is_unfilled),
        );
        cx.server(VisionSystems::See, Seeing::see);
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;

    use campfire_declarations::internals::BookLoad;
    use campfire_geometry::{Bounds, Grid, Polygon};
    use campfire_math::Num;
    use campfire_script::FailureKind;
    use campfire_script::{ActionCast, Budget, CallError, CallStart, Ctx, ScriptHost, ScriptRole};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{SimTick, StableId};
    use campfire_units::{SeenBy, UnitsWorld};
    use campfire_values::{ActionId, PackageIndex, Rank, Team, TeamSet};

    use crate::Vision;
    use crate::brush_map::BrushMap;
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

        /// Runs `ctx.reveal(at.position, radius, ms)` as `actor` casts, or as the mode with no
        /// actor, then applies what it queued; why it failed, when it did.
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
                Some(actor) => {
                    let cast = ActionCast {
                        action: ActionId::new(0),
                        rank: Rank::FIRST,
                        caster: actor,
                        allegiance: world.allegiance_of(actor).unwrap(),
                        package: PackageIndex::MODE,
                    };
                    CallStart::cast(cast, now)
                }
                None => CallStart::mode(ScriptRole::Mode, now),
            };
            ctx.begin_alone(world, start).unwrap();
            let at = ctx.view().unit(at).unwrap();
            let source = format!("fn probe(ctx, at) {{ ctx.reveal(at.position, {radius}, {ms}) }}");
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
