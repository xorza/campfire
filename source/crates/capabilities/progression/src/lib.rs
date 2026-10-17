// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use bevy_ecs::schedule::SystemSet;
use campfire_actions::{ActionsSet, EffectQueues};
use campfire_common::EnumAll;
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, SetPlace};
use campfire_sim::Capability;
use campfire_units::BodyUnit;

use crate::progression_column::RowReads;

mod error;
mod experience;
mod learn_orders;
mod learning;
mod level_ups;
mod points;
mod progression_action;
mod progression_api;
mod progression_column;
mod progression_effect;
mod track_book;
mod track_data;
mod track_set;

pub use crate::error::{ProgressionRefusal, TrackError};
pub use crate::experience::{Experience, Raised, TrackXp};
pub use crate::learn_orders::LearnOrders;
pub use crate::learning::Learning;
pub use crate::level_ups::{LevelUp, LevelUps};
pub use crate::points::Points;
pub use crate::progression_action::ProgressionAction;
pub use crate::progression_api::ProgressionApi;
pub use crate::progression_column::ProgressionColumn;
pub use crate::progression_effect::ProgressionEffect;
pub use crate::track_book::TrackBook;
pub use crate::track_data::{Thresholds, TrackData};
pub use crate::track_set::TrackSet;

/// The `progression` capability: experience on tracks, the levels it reaches, and the ranks
/// units learn for the points their levels give.
#[derive(Debug)]
pub struct Progression;

/// The steps of `progression`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ProgressionSet {
    /// The tick's learn orders learn their ranks.
    Commands,
}

impl ModuleSet for ProgressionSet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Commands => SetPlace::at(ActionsSet::Learn),
        }
    }
}

impl Module for Progression {
    const CAPABILITY: Option<Capability> = Some(Capability::Progression);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];
    type Set = ProgressionSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<Experience>();
        d.component::<Points>();
        d.resource::<LevelUps>();
        d.callback::<EffectQueues>(Capability::Progression, ProgressionEffect::queue_listed);
        d.book::<TrackBook>();
        d.column::<ProgressionColumn, RowReads>(ProgressionColumn::fill_row);
        d.api(ProgressionApi::register);
        d.book::<ByType<TrackSet>>();
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

    /// Adds progression to a match: in Inputs, the tick's learn orders learn their ranks. Its
    /// effects queue through its callback.
    fn install(cx: &mut InstallCx<'_, Self::Set>) {
        cx.add(ProgressionSet::Commands, LearnOrders::learn_ranks);
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_common::DeclaredName;
    use campfire_declare::Book;

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
