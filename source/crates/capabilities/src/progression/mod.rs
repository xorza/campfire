use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_sim::Capability;

use crate::actions::effect_queues::EffectQueues;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::phase::Phase;
use crate::declare::set_place::SetPlace;
use crate::progression::experience::Experience;
use crate::progression::learn_orders::LearnOrders;
use crate::progression::level_ups::LevelUps;
use crate::progression::points::Points;
use crate::progression::progression_api::ProgressionApi;
use crate::progression::progression_column::{ProgressionColumn, RowReads};
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::units::body_unit::BodyUnit;
use crate::units::by_type::ByType;

pub(crate) mod error;
pub(crate) mod experience;
pub(crate) mod learn_orders;
pub(crate) mod learning;
pub(crate) mod level_ups;
pub(crate) mod points;
pub(crate) mod progression_action;
pub(crate) mod progression_api;
pub(crate) mod progression_column;
pub(crate) mod progression_effect;
pub(crate) mod track_book;
pub(crate) mod track_data;
pub(crate) mod track_set;

/// The `progression` capability: experience on tracks, the levels it reaches, and the ranks
/// units learn for the points their levels give.
#[derive(Debug)]
pub struct Progression;

campfire_common::enum_all! {
    /// The steps of `progression`.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum ProgressionSet {
        /// The tick's learn orders learn their ranks.
        Commands,
    }
    /// Every step.
    pub(crate) const ALL;
}

impl ModuleSet for ProgressionSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Progression);
    const ALL: &'static [ProgressionSet] = &ProgressionSet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            ProgressionSet::Commands => Phase::Learn,
        })
    }
}

impl Module for Progression {
    const CAPABILITY: Option<Capability> = Some(Capability::Progression);
    const NEEDS: &'static [Capability] = &[Capability::Stats];

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
            (!tracks.is_empty()).then(|| {
                Experience::new(
                    tracks,
                    at.world()
                        .get_resource::<TrackBook>()
                        .and_then(TrackBook::level_track),
                )
            })
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
        cx.schedule
            .add_systems(LearnOrders::learn_ranks.in_set(ProgressionSet::Commands));
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;

    use crate::books::Books;
    use crate::progression::Progression;
    use crate::progression::track_book::TrackBook;
    use crate::progression::track_data::TrackData;
    use crate::values::declared_name::DeclaredName;

    impl Progression {
        /// Loads the mode's `tracks`, which hold its rules, into the match.
        pub(crate) fn load_tracks(world: &mut World, tracks: &BTreeMap<DeclaredName, TrackData>) {
            Books::load(world, TrackBook::new(tracks).unwrap());
        }
    }
}

#[cfg(test)]
mod tests;
