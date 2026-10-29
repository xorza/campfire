//! Moves one entity between the prediction timeline and the interpolation timeline when gameplay
//! inserts a [`TimelineSwitch`](crate::timeline_switch::TimelineSwitch), so a remote player near
//! enough to touch is predicted while the rest of the world is interpolated.

// A request is handled over two frames. Say the request is inserted on frame F
// (gameplay code usually inserts it from `Update`):
//
// 1. `TimelineSwitchPlugin::save_previous_visuals` runs in `PostUpdate` of frame F, after frame
//    interpolation and after the correction apply, so what it reads is what the
//    frame shows. It saves that value as
//    [`PreviousVisual`](crate::previous_visual::PreviousVisual) — for a switch to the
//    interpolation timeline, the only thing that can record it — and resolves
//    the request into a `PendingSwitch`: the timeline it names (or the one a
//    flip picks) and the blend schedule to run, committed for the next frame.
//    The request itself is dropped, so what is carried to the next frame is the
//    resolution and nothing else. The entity's timeline does not change yet. A
//    switch that snaps saves nothing.
// 2. `TimelineSwitchPlugin::apply_saved_switches` runs in `PreUpdate` of frame F+1, after
//    replication receive and before the rollback check. It swaps the
//    [`Predicted`] / [`Interpolated`] markers and removes the request. For a
//    switch to the prediction timeline it also asks for a forced rollback, so
//    that in this same frame the world rewinds to K and replays under the new
//    marker, leaving fresh simulated values in place.
// 3. The creation system of each correction in the `correction` module records the
//    blend's error in `PostUpdate` of frame F+1, after frame interpolation: the jump
//    from the value saved in step 1 to whatever the destination timeline has
//    now. It runs every frame, and keeps the window's error up to date for as
//    long as the window is live.
//
// `TimelineSwitchPlugin::expire_switch_blends` ends the window and clears what it kept.
//
// # How a blend relates to `VisualCorrection`
//
// A blend *is* a correction. There is one error per corrected component, one
// record of one jump:
//
// ```text
// rendered = destination + error    (the apply adds the error every frame)
// error    = previous - destination (recorded once, when the window opens)
// ```
//
// and one decay that gives it up. The switch does not own a second error and
// does not stack its error on top of a correction: it records the same kind of
// measurement the rollback machinery records, in the same
// [`PreviousVisual`](crate::previous_visual::PreviousVisual) slot, and its window
// decides how that error is decayed. When a new correction is applied while
// a previous one was running, the highest decay ratio between the two is used
// (the one that would give the biggest error).

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_time::{Time, Virtual};
use campfire_log::LogEvent;
use campfire_net_frame_interpolation::FrameInterpolate;
use campfire_net_frame_interpolation::FrameRegistry;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::Interpolated;
use campfire_net_replication::Predicted;
use campfire_net_replication::ReplicationSystems;
use tracing::trace;

use crate::on_timeline::OnTimeline;
use crate::pending_switch::PendingSwitch;
use crate::prediction_registry::PredictionRegistry;
use crate::prediction_systems::PredictionSystems;
use crate::rollback_check_state::RollbackCheckState;
use crate::rollback_systems::RollbackSystems;
use crate::switch_blend::SwitchBlend;
use crate::switch_direction::SwitchDirection;
use crate::switch_refused::SwitchRefused;
use crate::timeline_switch::TimelineSwitch;
use crate::timeline_switch_setup::TimelineSwitchSetup;

/// The timeline-switch pipeline on the prediction client. Each of its systems is in
/// [`PredictionSystems::All`], which runs only for client topologies: host-servers stay
/// authoritative and never switch.
#[derive(Debug)]
pub(crate) struct TimelineSwitchPlugin;

impl Plugin for TimelineSwitchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TimelineSwitchSetup>();
        app.add_systems(
            PreUpdate,
            Self::apply_saved_switches
                .after(ReplicationSystems::Receive)
                .in_set(PredictionSystems::Rollback)
                .before(RollbackSystems::Check),
        );
        app.add_systems(
            PostUpdate,
            (
                // The correction that starts a blend is recorded by each
                // correction's creation system in the `correction` module, which runs
                // between frame interpolation and the correction apply.
                //
                // A window is lifted before new requests are looked at, so a request
                // that arrives on the frame its window expires is served then rather
                // than a frame later. Without this the two systems have no relative
                // order, and which one runs first would decide that.
                Self::expire_switch_blends
                    .in_set(PredictionSystems::All)
                    .after(RollbackSystems::VisualCorrection)
                    .before(Self::save_previous_visuals),
                // Saving reads what the frame renders, so it runs after the apply
                // has added this frame's correction.
                Self::save_previous_visuals
                    .in_set(PredictionSystems::All)
                    .after(RollbackSystems::VisualCorrection),
            ),
        );
    }
}

impl TimelineSwitchPlugin {
    /// Resolves every switch requested this frame, which each correction's save then saves the
    /// value on screen for.
    ///
    /// Runs in `PostUpdate` after frame interpolation and after the correction
    /// apply, so what the saves read is what this frame renders, whatever point of the
    /// frame the request was inserted from.
    ///
    /// Requests for entities that are already blending, or that already have a
    /// committed switch waiting for its apply pass, are dropped.
    pub(crate) fn save_previous_visuals(
        setup: Res<'_, TimelineSwitchSetup>,
        mut requests: Query<
            '_,
            '_,
            (
                Entity,
                &TimelineSwitch,
                Has<Predicted>,
                Has<Interpolated>,
                Has<SwitchBlend>,
                Has<PendingSwitch>,
            ),
        >,
        mut commands: Commands<'_, '_>,
        mut saves: Deferred<'_, DeferredEntityCommands>,
    ) {
        for (entity, switch, is_predicted, is_interpolated, is_blending, is_committed) in
            &mut requests
        {
            // Saving while a blend is running would fight the window that is still
            // converging, so the request waits for it to end. A policy that
            // re-inserts every frame produces this routinely, so it is not a user
            // error; it is also what makes a repeated request harmless.
            if is_blending {
                trace!(?entity, "dropping switch while a blend is active");
                commands.entity(entity).remove::<TimelineSwitch>();
                continue;
            }
            // A committed switch holds the entity until it is applied. Resolving a
            // request on top of it would clobber the resolution the save pass already
            // recorded a value for, leaving that value to be measured as a jump that
            // never happened.
            if is_committed {
                trace!(
                    ?entity,
                    "dropping switch while one is waiting to be applied"
                );
                commands.entity(entity).remove::<TimelineSwitch>();
                continue;
            }
            // A flip resolves against the markers the entity has now.
            let pending =
                match switch.resolve(OnTimeline::of(is_predicted, is_interpolated), &setup) {
                    Ok(pending) => pending,
                    Err(rejection) => {
                        SwitchRefused {
                            entity: entity.to_bits(),
                            rejection,
                        }
                        .log();
                        commands.entity(entity).remove::<TimelineSwitch>();
                        continue;
                    }
                };
            // Nothing is saved for a switch that snaps: with no window there is
            // nothing to measure a correction from, and a saved value would be left
            // behind with no reader. A switch to the prediction timeline still gets
            // the rollback's own correction, which is what a snap wants.
            let total = pending.total;
            let direction = pending.direction;
            saves.remove::<TimelineSwitch>(entity);
            saves.insert(entity, pending);
            trace!(?entity, ?direction, ?total, "saved switch visual");
        }
    }

    /// Swaps the timeline markers of every switch the save pass committed last frame.
    pub(crate) fn apply_saved_switches(
        time: Res<'_, Time<Virtual>>,
        mut pending: Query<'_, '_, (Entity, &PendingSwitch)>,
        metadata: Option<ResMut<'_, RollbackCheckState>>,
        frame_registry: Res<'_, FrameRegistry>,
        registry: Res<'_, PredictionRegistry>,
        mut commands: Commands<'_, '_>,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) {
        if pending.is_empty() {
            return;
        }
        let now = time.elapsed();
        let mut forced_rollback = false;
        for (entity, switch) in &mut pending {
            match switch.direction {
                SwitchDirection::ToPredicted => {
                    deferred.remove::<Interpolated>(entity);
                    deferred.insert(entity, Predicted);
                    // A predicted entity's visual blends its last two fixed ticks under
                    // `FrameInterpolate`. (The saved value requires it too; on a snapping
                    // switch there is no saved value, hence the explicit insert.)
                    deferred.insert(entity, FrameInterpolate);
                    // Forced rollback at K: the rollback check below consumes this
                    // before its policy scan, rewinds the world to confirmed data at
                    // K, and replays forward under the new marker, so the window
                    // blends toward fresh simulation. Before the first sync there is
                    // no K to rewind to, so the switch proceeds without a force and
                    // the next natural rollback corrects.
                    forced_rollback = true;
                }
                SwitchDirection::ToInterpolated => {
                    deferred.remove::<Predicted>(entity);
                    deferred.insert(entity, Interpolated);
                    // Delayed interpolation writes the component every render frame, which
                    // the visual then shows as it is, so no fixed tick's history blends into
                    // it.
                    deferred.remove::<FrameInterpolate>(entity);
                }
            }
            // The frame histories belong to the era the entity leaves: they froze while it was
            // interpolated, and once it is interpolated nothing reads them. Kept, the next
            // predicted era's first frames would blend from a stale value; dropped, they re-seed
            // from the live one.
            let mut entity_commands = commands.entity(entity);
            for &history in frame_registry.frame_history_component_ids() {
                entity_commands.remove_by_id(history);
            }
            // The prediction histories are the predicted era's too. Kept on an interpolated entity,
            // prediction would go on recording into them and cutting, at its own rollback window,
            // the confirmed history that interpolation reads further back.
            if matches!(switch.direction, SwitchDirection::ToInterpolated) {
                for history in registry.prediction_history_ids() {
                    entity_commands.remove_by_id(history);
                }
            }
            // The switch has been served: it is one-shot, so the committed state goes
            // with the markers it swapped.
            deferred.remove::<PendingSwitch>(entity);
            if switch.blends() {
                deferred.insert(entity, SwitchBlend::new(now, switch.total, switch.ease));
            }
            trace!(
                ?entity,
                direction = ?switch.direction,
                blends = switch.blends(),
                "applied timeline switch"
            );
        }
        if forced_rollback
            && let Some(mut metadata) = metadata
            && let Some(k) = metadata.last_processed_confirmed_tick()
        {
            metadata.request_switch_rollback(k);
            trace!(
                rollback_tick = k.0,
                "timeline switch requested forced rollback"
            );
        }
    }

    /// Lifts expired blend markers. Each correction has cleared the window's state first: the
    /// window is over, so the values it blended from have no reader left, and a component the
    /// destination never presented would otherwise keep them on the entity for good.
    pub(crate) fn expire_switch_blends(
        time: Res<'_, Time<Virtual>>,
        blends: Query<'_, '_, (Entity, &SwitchBlend)>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = time.elapsed();
        for (entity, blend) in &blends {
            if blend.is_expired(now) {
                commands.entity(entity).remove::<SwitchBlend>();
            }
        }
    }
}

#[cfg(test)]
mod tests;
