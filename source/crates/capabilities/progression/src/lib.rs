// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

use bevy_ecs::schedule::SystemSet;
use campfire_actions::ActionsSystems;
use campfire_common::EnumAll;
use campfire_declarations::{ById, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_script::{ApiVersion, EventPhase, Status};
use campfire_sim::{Capability, SimEdgeSystems, SimSystems};
use campfire_stats::StatsSystems;
use campfire_units::{BodyUnit, TrackLevels, UnitGrid};
use campfire_values::{TrackId, UnitType};

use crate::experience_book::ExperienceShare;
use crate::progression_column::RowReads;

mod error;
mod experience;
mod experience_book;
mod experience_data;
mod learn_orders;
mod learning;
mod level_ups;
mod mode_tracks_data;
mod points;
mod progression_api;
mod progression_books;
mod progression_column;
mod progression_directive;
mod progression_effect;
mod track_book;
mod track_data;
mod track_set;
mod type_tracks_data;
mod xp_effect_data;

pub use crate::error::{ExperienceError, ProgressionRefusal, TrackError};
pub use crate::experience::{Experience, TrackRaise, TrackXp};
pub use crate::experience_book::ExperienceBook;
pub use crate::experience_data::{ExperienceData, ExperienceTo, Gives, ModeExperienceData};
pub use crate::learn_orders::LearnOrders;
pub use crate::learning::Learning;
pub use crate::level_ups::{LevelUp, LevelUps, OnLevelUp};
pub use crate::mode_tracks_data::ModeTracksData;
pub use crate::points::Points;
pub use crate::progression_api::ProgressionApi;
pub use crate::progression_column::ProgressionColumn;
pub use crate::progression_directive::ProgressionDirective;
pub use crate::track_book::TrackBook;
pub use crate::track_data::{Thresholds, TrackData};
pub use crate::track_set::TrackSet;
pub use crate::type_tracks_data::TypeTracksData;
pub use crate::xp_effect_data::XpEffectData;

/// The `progression` capability: experience on tracks, the levels it reaches, and the ranks
/// units learn for the points their levels give.
#[derive(Debug)]
pub struct Progression;

/// The steps of `progression`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ProgressionSystems {
    /// The tick's learn orders learn their ranks.
    Commands,
    /// Once units died, after the gap's stats refresh: their deaths give experience, whose
    /// level-ups the mode answers in its stage.
    Share,
}

impl ModuleSystems for ProgressionSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Commands => SetPlace::at(ActionsSystems::Learn),
            Self::Share => SetPlace::gap(SimEdgeSystems::After(SimSystems::Resolve)).after(
                StatsSystems::Refresh(SimEdgeSystems::After(SimSystems::Resolve)),
            ),
        }
    }
}

impl Module for Progression {
    const CAPABILITY: Option<Capability> = Some(Capability::Progression);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];
    type Systems = ProgressionSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<TypeTracksData>();
        d.section::<ModeTracksData>();
        d.component::<Experience>();
        d.derived::<TrackLevels>();
        d.server_part::<Experience, TrackLevels>();
        d.component::<Points>();
        d.resource::<LevelUps>();
        d.mode_events::<LevelUps>(EventPhase::Gains);
        d.listed::<XpEffectData>();
        d.book::<TrackBook>(TrackBook::build);
        d.section::<ModeExperienceData>();
        d.section::<ExperienceData>();
        d.book::<ExperienceBook>(ExperienceBook::build);
        d.rule(Self::levels_rule);
        d.names::<TrackId>(Self::track_names);
        d.column::<ProgressionColumn, RowReads>(ProgressionColumn::fill_row);
        d.api(ProgressionApi::register);
        d.hook::<OnLevelUp>(Status::Runs(ApiVersion::FIRST));
        d.book::<ById<UnitType, TrackSet>>(TrackSet::book);
        d.part::<BodyUnit, Experience>(|at| {
            let tracks = *at.of_type::<TrackSet>()?;
            (!tracks.is_empty()).then(|| Experience::new(tracks))
        });
        d.part::<BodyUnit, Points>(|at| {
            Points::at_spawn(
                *at.of_type::<TrackSet>()?,
                at.world()
                    .get_resource::<TrackBook>()
                    .and_then(TrackBook::level_track),
            )
        });
    }

    /// Adds progression to a match: in Inputs, the tick's learn orders learn their ranks, and on a
    /// server each unit shows its levels as its experience changed. Its effects queue through its
    /// callback.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.add(ProgressionSystems::Commands, LearnOrders::learn_ranks);
        cx.server(ProgressionSystems::Commands, Experience::show_levels);
        if cx
            .book::<ExperienceBook>()
            .and_then(ExperienceBook::share)
            .is_some()
        {
            cx.server(
                ProgressionSystems::Share,
                UnitGrid::before_reading(ExperienceShare::give),
            );
        }
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_common::DeclaredName;

    use campfire_declarations::internals::BookLoad;

    use crate::Progression;
    use crate::track_book::TrackBook;
    use crate::track_data::TrackData;

    impl Progression {
        /// Loads the mode's `tracks`, which hold its rules, into the match.
        pub fn load_tracks(world: &mut World, tracks: &BTreeMap<DeclaredName, TrackData>) {
            TrackBook::new(tracks).unwrap().load_into(world);
        }
    }
}
