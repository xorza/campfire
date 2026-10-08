use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::actions::capability_does::CapabilityDoes;
use crate::progression::experience::Experience;
use crate::progression::level_ups::{LevelUp, LevelUps};
use crate::progression::points::Points;
use crate::progression::progression_column::ProgressionColumn;
use crate::progression::track_book::TrackBook;
use crate::scripts::effects::Effect;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::frame::Frame;
use crate::stats::level::Level;
use crate::units::script_view::View;
use crate::units::track_id::TrackId;
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::world::Mut;
use campfire_sim::EntityIndex;

/// A change to units' progress that a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressionEffect {
    /// Experience, not negative, on a track the unit has.
    AddXp {
        unit: StableId,
        track: TrackId,
        amount: Num,
    },
}

impl ProgressionEffect {
    /// Queues the listed `does`, experience, to `unit` in `frame`; a unit `view` does not hold,
    /// or that does not have the track, fails the call, as `ctx.add_xp` does.
    pub(crate) fn queue_listed(
        does: CapabilityDoes,
        unit: StableId,
        _: Option<StableId>,
        frame: &mut Frame,
        view: &View,
    ) -> Result<(), CallError> {
        let CapabilityDoes::Xp { track, amount } = does else {
            unreachable!("progression queues only its own listed effects")
        };
        if !ProgressionColumn::has(view, unit, track) {
            return Err(CallError::Api(ApiError::NoTrack));
        }
        frame.effects.push(ProgressionEffect::AddXp {
            unit,
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
    fn apply(self, world: &mut World, _: &mut Frame, _: Tick) {
        match self {
            ProgressionEffect::AddXp {
                unit,
                track,
                amount,
            } => {
                let entity = world
                    .resource::<EntityIndex>()
                    .get(unit)
                    .expect("a unit given experience exists");
                world.resource_scope(|world, book: Mut<'_, TrackBook>| {
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
                        .extend(reached.map(|level| LevelUp { unit, track, level }));
                });
            }
        }
    }
}
