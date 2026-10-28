// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

use bevy_ecs::schedule::SystemSet;
use campfire_actions::ActionsSystems;
use campfire_common::EnumAll;
use campfire_declarations::{ById, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_players::Player;
use campfire_script::{ApiVersion, EventPhase, Status};
use campfire_sim::{Capability, SimEdgeSystems, SimSystems};
use campfire_stats::StatsSystems;
use campfire_units::{BodyUnit, SpawnCallbacks, TrackLevels, UnitGrid};
use campfire_values::{PerkId, TrackId, UnitType};

use crate::experience_book::ExperienceShare;
use crate::progression_column::RowReads;

mod error;
mod experience;
mod experience_book;
mod experience_data;
mod learn_orders;
mod learning;
mod level_ups;
mod mode_perks_data;
mod mode_tracks_data;
mod perk_book;
mod perk_data;
mod player_experience;
mod player_perks;
mod player_points;
mod points;
mod progression_api;
mod progression_books;
mod progression_call;
mod progression_column;
mod progression_directive;
mod progression_effect;
mod track_book;
mod track_data;
mod track_modifier_book;
mod track_set;
mod type_tracks;
mod type_tracks_data;
mod xp_effect_data;

pub use crate::error::{ExperienceError, PerkError, ProgressionRefusal, TrackError};
pub use crate::experience::{Experience, TrackRaise, TrackXp};
pub use crate::experience_book::ExperienceBook;
pub use crate::experience_data::{
    ExperienceData, ExperienceTo, Gives, ModeExperienceData, ShareData,
};
pub use crate::learn_orders::LearnOrders;
pub use crate::learning::Learning;
pub use crate::level_ups::{LevelUp, LevelUps, OnLevelUp};
pub use crate::mode_perks_data::ModePerksData;
pub use crate::mode_tracks_data::ModeTracksData;
pub use crate::perk_book::{Learner, PerkBook};
pub use crate::perk_data::PerkData;
pub use crate::player_experience::PlayerExperience;
pub use crate::player_perks::PlayerPerks;
pub use crate::player_points::PlayerPoints;
pub use crate::points::Points;
pub use crate::progression_api::ProgressionApi;
pub use crate::progression_call::ProgressionCall;
pub use crate::progression_column::ProgressionColumn;
pub use crate::progression_directive::ProgressionDirective;
pub use crate::track_book::TrackBook;
pub use crate::track_data::{Thresholds, TrackData, TrackHolder, TrackThresholds};
pub use crate::track_modifier_book::TrackModifierBook;
pub use crate::track_set::TrackSet;
pub use crate::type_tracks::TypeTracks;
pub use crate::type_tracks_data::{StartData, TypeTrackData, TypeTracksData};
pub use crate::xp_effect_data::XpEffectData;

/// The `progression` capability: experience on tracks, the levels it reaches, the ranks units
/// learn for the points their levels give, and the perks players learn for theirs.
#[derive(Debug)]
pub struct Progression;

/// The steps of `progression`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ProgressionSystems {
    /// The tick's learn orders learn their ranks and perks.
    Commands,
    /// Once units died, after the gap's stats refresh: their deaths give experience, whose
    /// level-ups the mode answers in its stage.
    Share,
    /// Before the grant pass of the tick's end: the units whose experience changed push the
    /// grants of their levels' modifiers.
    Grant,
}

impl ModuleSystems for ProgressionSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Commands => SetPlace::at(ActionsSystems::Learn),
            Self::Share => SetPlace::gap(SimEdgeSystems::After(SimSystems::Resolve)).after(
                StatsSystems::Refresh(SimEdgeSystems::After(SimSystems::Resolve)),
            ),
            Self::Grant => SetPlace::stage(SimSystems::Vision).before(StatsSystems::GrantAtEnd),
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
        d.component::<PlayerExperience>();
        d.component::<PlayerPoints>();
        d.part::<Player, PlayerExperience>(|at| {
            let tracks = at.world().get_resource::<TrackBook>()?.player_tracks();
            (!tracks.is_empty()).then(|| PlayerExperience(Experience::new(tracks)))
        });
        d.part::<Player, PlayerPoints>(|at| {
            PlayerPoints::at_start(at.world().get_resource::<TrackBook>()?)
        });
        d.section::<ModePerksData>();
        d.book::<PerkBook>(PerkBook::build);
        d.names::<PerkId>(Self::perk_names);
        d.component::<PlayerPerks>();
        d.part::<Player, PlayerPerks>(|at| {
            let book = at.world().get_resource::<PerkBook>()?;
            (!book.is_empty()).then(PlayerPerks::default)
        });
        d.resource::<LevelUps>();
        d.mode_events::<LevelUps>(EventPhase::Gains);
        d.listed::<XpEffectData>();
        d.book::<TrackBook>(TrackBook::build);
        d.book::<TrackModifierBook>(TrackModifierBook::build);
        d.section::<ModeExperienceData>();
        d.section::<ExperienceData>();
        d.book::<ExperienceBook>(ExperienceBook::build);
        d.rule(Self::levels_rule);
        d.callback::<SpawnCallbacks>(Capability::Progression, LevelUps::start);
        d.names::<TrackId>(Self::track_names);
        d.column::<ProgressionColumn, RowReads>(ProgressionColumn::fill_row);
        d.call_extension::<ProgressionCall>();
        d.api(ProgressionApi::register);
        d.hook::<OnLevelUp>(Status::Runs(ApiVersion::FIRST));
        d.book::<ById<UnitType, TypeTracks>>(TypeTracks::book);
        d.part::<BodyUnit, Experience>(|at| {
            let tracks = at.of_type::<TypeTracks>()?.set();
            (!tracks.is_empty()).then(|| Experience::new(tracks))
        });
        d.part::<BodyUnit, Points>(|at| {
            Points::at_spawn(
                at.of_type::<TypeTracks>()?.set(),
                at.world().get_resource::<TrackBook>()?,
            )
        });
    }

    /// Adds progression to a match: in Inputs, the tick's learn orders learn their ranks, and on a
    /// server their perks, in a mode of perks, and each unit shows its levels as its experience
    /// changed. Its effects queue through its callback.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.add(ProgressionSystems::Commands, LearnOrders::learn_ranks);
        if cx.book::<PerkBook>().is_some_and(|book| !book.is_empty()) {
            cx.server(ProgressionSystems::Commands, LearnOrders::learn_perks);
        }
        cx.server(ProgressionSystems::Commands, Experience::show_levels);
        if cx
            .book::<TrackModifierBook>()
            .is_some_and(TrackModifierBook::any)
        {
            cx.server(ProgressionSystems::Grant, TrackModifierBook::want);
        }
        if cx
            .book::<ExperienceBook>()
            .is_some_and(ExperienceBook::shares)
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

    use campfire_stats::ModifierBook;
    use campfire_units::Units;
    use campfire_values::{ModifierId, PackageIndex, UnitType};

    use crate::Progression;
    use crate::track_book::TrackBook;
    use crate::track_data::TrackData;
    use crate::track_modifier_book::TrackModifierBook;
    use crate::type_tracks::TypeTracks;
    use crate::type_tracks_data::TypeTracksData;

    impl Progression {
        /// Loads the mode's `tracks`, which hold its rules, into the match, their modifiers of
        /// the mode's package, which the match holds.
        pub fn load_tracks(world: &mut World, tracks: &BTreeMap<DeclaredName, TrackData>) {
            TrackBook::new(tracks).unwrap().load_into(world);
            let book = world.get_resource::<ModifierBook>();
            let modifiers = tracks.values().map(|track| {
                let named = track.modifiers.iter().map(|name| {
                    let book = book.expect("a match of track modifiers holds modifiers");
                    book.names()
                        .named((PackageIndex::MODE, name.as_str()))
                        .expect("a track's modifier loaded")
                });
                named.collect::<Box<[ModifierId]>>()
            });
            TrackModifierBook::new(modifiers.collect()).load_into(world);
        }

        /// Gives `unit_type` of the mode the tracks `data` names, as its `tracks` would, of the
        /// tracks and modifiers the match holds.
        pub fn give_type_tracks(world: &mut World, unit_type: UnitType, data: &TypeTracksData) {
            let tracks = TypeTracks::resolve(
                data,
                world.resource::<TrackBook>(),
                world.resource::<ModifierBook>(),
                PackageIndex::MODE,
            )
            .expect("a test's tracks resolve");
            Units::give_type(world, unit_type, tracks);
        }
    }
}
