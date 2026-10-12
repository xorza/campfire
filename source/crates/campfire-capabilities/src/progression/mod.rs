use campfire_sim::Capability;

use crate::actions::effect_queues::EffectQueues;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::progression::experience::Experience;
use crate::progression::level_ups::LevelUps;
use crate::progression::points::Points;
use crate::progression::progression_api::ProgressionApi;
use crate::progression::progression_column::{ProgressionColumn, RowParts};
use crate::progression::progression_effect::ProgressionEffect;
use crate::progression::track_book::TrackBook;

pub(crate) mod experience;
pub(crate) mod level_ups;
pub(crate) mod points;
pub(crate) mod progression_api;
pub(crate) mod progression_column;
pub(crate) mod progression_effect;
pub(crate) mod progression_world;
pub(crate) mod track_book;
pub(crate) mod track_data;
pub(crate) mod track_set;

/// The `progression` capability: experience on tracks, and the levels it reaches.
#[derive(Debug)]
pub struct Progression;

impl Declares for Progression {
    const MODULE: Option<Capability> = Some(Capability::Progression);

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<Experience>();
        d.component::<Points>();
        d.resource::<LevelUps>();
        d.book::<TrackBook>();
        d.column::<ProgressionColumn, RowParts>(ProgressionColumn::fill_row);
        d.api(ProgressionApi::register);
    }
}

impl Progression {
    /// Adds progression to a match, with no track until the mode loads its own.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        let world = &mut *cx.world;
        world.insert_resource(LevelUps::default());
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Progression, ProgressionEffect::queue_listed);
    }
}

#[cfg(test)]
mod tests;
