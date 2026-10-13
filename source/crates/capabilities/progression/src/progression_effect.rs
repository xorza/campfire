use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::world::World;
use campfire_actions::CapabilityDoesSpec;
use campfire_core::{ApiError, CallError, Effect, Frame, TrackId, View};
use campfire_math::Num;
use campfire_sim::{SimWorld, StableId};
use campfire_stats::Level;

use crate::experience::Experience;
use crate::level_ups::{LevelUp, LevelUps};
use crate::points::Points;
use crate::progression_column::ProgressionColumn;
use crate::track_book::TrackBook;

/// A change to units' progress that a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressionEffect {
    /// Experience, not negative, on a track the unit has.
    AddXp {
        id: StableId,
        track: TrackId,
        amount: Num,
    },
}

impl ProgressionEffect {
    /// Queues the listed `does`, experience, to `id` in `frame`; a unit `view` does not hold,
    /// or that does not have the track, fails the call, as `ctx.add_xp` does.
    pub(crate) fn queue_listed(
        does: CapabilityDoesSpec,
        id: StableId,
        _: Option<StableId>,
        frame: &mut Frame,
        view: &View,
    ) -> Result<(), CallError> {
        let CapabilityDoesSpec::Xp { track, amount } = does else {
            unreachable!("progression queues only its own listed effects")
        };
        if !ProgressionColumn::has(view, id, track) {
            return Err(CallError::Api(ApiError::NoTrack));
        }
        frame.effects.push(ProgressionEffect::AddXp {
            id,
            track,
            amount: amount.number(frame),
        });
        Ok(())
    }
}

impl Effect for ProgressionEffect {
    /// Applies the effect: experience raises its track's level, each level reached joins the
    /// tick's level-ups, and a level reached on the `level` track becomes the unit's level and
    /// gives it a point.
    fn apply(self, world: &mut World, _: &mut Frame) {
        match self {
            ProgressionEffect::AddXp { id, track, amount } => {
                let entity = world.entity_of(id).expect("a unit given experience exists");
                let book = world.resource::<TrackBook>().clone();
                let mut carrier = world.entity_mut(entity);
                let (mut experience, mut level, points) = carrier
                    .get_components_mut::<(
                        &mut Experience,
                        Option<&mut Level>,
                        Option<&mut Points>,
                    )>()
                    .expect("a unit given experience has tracks");
                // The unit's level counts as changed only when it rises, as its stats are
                // derived again when it changes.
                let mut unit_level = level.as_deref().copied();
                let raised = experience.add(track, amount, &book, unit_level.as_mut());
                if let (Some(level), Some(value)) = (&mut level, unit_level) {
                    level.set_if_neq(value);
                }
                if raised.to == raised.from {
                    return;
                }
                if level.is_some() && book.level_track() == Some(track) {
                    points
                        .expect("a unit with the `level` track has points")
                        .gain(raised.to.get() - raised.from.get());
                }
                let reached = (raised.from.get() + 1..=raised.to.get())
                    .map(|level| Level::new(level).expect("a level past another"));
                let mut level_ups = world.resource_mut::<LevelUps>();
                level_ups
                    .0
                    .extend(reached.map(|level| LevelUp { id, track, level }));
            }
        }
    }
}
