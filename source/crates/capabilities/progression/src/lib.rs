#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_actions::EffectQueues;
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_derive::EnumAll;
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
pub(crate) enum ProgressionSet {
    /// The tick's learn orders learn their ranks.
    Commands,
}

impl ModuleSet for ProgressionSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Progression);
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            Self::Commands => Phase::Learn,
        })
    }
}

impl Module for Progression {
    const CAPABILITY: Option<Capability> = Some(Capability::Progression);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<ProgressionSet>();
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
    fn install(cx: &mut InstallCx<'_>) {
        cx.systems(LearnOrders::learn_ranks.in_set(ProgressionSet::Commands));
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_declare::Book;
    use campfire_values::DeclaredName;

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
