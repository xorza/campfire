//! Client-side visual correction for predicted components.
//!
//! Prediction rollback has two separate goals:
//! - the simulation must immediately use the corrected state produced by
//!   rollback and replay;
//! - the rendered value should not snap from the pre-rollback visual state to
//!   the corrected visual state in one frame.
//!
//! Correction is installed for components registered with `correct_linear`,
//! `correct_with`, or `correct_custom`. A built-in correction adds typed systems
//! for its component `C` and its error `D`, which record, decay and clear its
//! [`VisualCorrection`].
//!
//! # How a correction is recorded
//!
//! Every correction is one measurement of one jump: the value that was on screen
//! before it, against the value that is on screen now.
//!
//! ```text
//! rendered = live + error        (the apply adds the error every frame)
//! error    = previous - live     (recorded once, per jump)
//! ```
//!
//! The value before the jump is saved as [`PreviousVisual`], and the value now is
//! whatever the live component holds when the correction is recorded. Recording
//! happens in `PostUpdate`, after [`FrameInterpolationSystems::Interpolate`] has
//! written the value the frame renders, so the measurement is against exactly
//! what the entity would show without a correction. Nothing needs to know how
//! that value was produced: the frame-interpolation rules, including bundle
//! rules and the fixed overstep, have already run.
//!
//! That is also why there is no correction-time sampling of its own. Correction
//! used to re-run the frame-interpolation rules at `EndRollback` to synthesize
//! the value frame interpolation was about to write; now it simply runs after the
//! real thing. A timeline switch is the same measurement with a different value
//! before the jump (see [`crate::switch`]), so it goes through this same system.
//!
//! # How the error is given up
//!
//! [`VisualCorrection`] holds the error and the clock reading it was recorded at.
//! `update_visual_correction` multiplies the error by a keep ratio each frame:
//!
//! - on the entity's [`CorrectionPolicy`], or the global one on
//!   [`PredictionManager`], which defaults to an unbounded exponential and can
//!   be set to any [`CorrectionEase`](crate::correction_ease::CorrectionEase) with its own duration;
//! - on a live [`SwitchBlend`] window's curve instead, when the entity has one.
//!
//! The two combine by taking whichever gives the error up more slowly at that
//! instant, so a blend is never shortened by the policy and a jump arriving late
//! in a window is never dumped by the curve's tail. A correction that has
//! converged is dropped rather than carried.
//!
//! # The rest of the frame
//!
//! - Frame interpolation reads a predicted `C` from its [`PredictionHistory`],
//!   which prediction registers as `C`'s frame source: its value at the last
//!   tick and the tick before.
//! - [`FrameInterpolationSystems::Restore`] runs in `RunFixedMainLoop` before
//!   fixed simulation and restores the live component `C` to its value at the
//!   last tick, so fixed systems read simulation state, not the previous frame's
//!   visual interpolation.
//! - [`FrameInterpolationSystems::Interpolate`] runs in `PostUpdate` and writes
//!   the visual `C` by interpolating those two values with the current fixed
//!   overstep.
//! - Rollback runs in `PreUpdate`. Just before a live predicted `C` is
//!   overwritten, rollback saves [`PreviousVisual`] if `C` has correction
//!   enabled, whatever it is about to do to the component. Replay then advances
//!   the live component to the corrected simulation value for the current tick,
//!   and rewrites the prediction history the frame interpolation reads, so
//!   nothing repairs a frame history after it.
//!
//! [`PreviousVisual`] is only cleared by the system that records the correction,
//! so a component that is removed and added back still has its jump measured, and
//! a component that never comes back does not leave its saved value behind.
//!
//! Both [`PreviousVisual`] and [`VisualCorrection`] are slots that go on with the
//! prediction history and hold `None` while idle: a rollback and a correction write
//! them in place, and move no entity between tables.
use crate::correction_policy::CorrectionPolicy;
use crate::pending_switch::PendingSwitch;
use crate::prediction_history::PredictionHistory;
use crate::prediction_manager::PredictionManager;
use crate::prediction_systems::PredictionSystems;
use crate::previous_visual::PreviousVisual;
use crate::rollback_systems::RollbackSystems;
use crate::switch;
use crate::switch_blend::SwitchBlend;
use crate::switch_direction::SwitchDirection;
use crate::visual_correction::{CONVERGED, VisualCorrection, VisualError};
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_time::{Time, Virtual};
use bevy_utils::prelude::DebugName;
use campfire_net_interpolation::FrameInterpolationSystems;
use campfire_net_interpolation::HistoryComponent;
use campfire_net_interpolation::LerpFn;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::Diffable;
use core::fmt::Debug;
use tracing::trace;

/// The correction fn of component `C` on errors of type `D`, which its `(C, D)` correction
/// system reads typed.
#[derive(Resource)]
pub(crate) struct CorrectionFn<C, D> {
    correction_fn: LerpFn<D>,
    marker: core::marker::PhantomData<fn() -> C>,
}

impl<C, D> CorrectionFn<C, D> {
    pub(crate) const fn new(correction_fn: LerpFn<D>) -> Self {
        Self {
            correction_fn,
            marker: core::marker::PhantomData,
        }
    }
}

impl<C, D> Debug for CorrectionFn<C, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CorrectionFn").finish_non_exhaustive()
    }
}

/// Installs built-in visual correction systems for predicted component `C`.
///
/// The corrections are recorded in [`PostUpdate`], after frame interpolation,
/// and the visual error decays in [`PostUpdate`], in
/// [`RollbackSystems::VisualCorrection`].
pub(crate) fn add_correction_systems<
    C: HistoryComponent + Diffable<D>,
    D: Default + Clone + Debug + Send + Sync + 'static,
>(
    app: &mut App,
) {
    app.add_systems(
        PostUpdate,
        (
            create_visual_correction::<C, D>
                // The measurement is `previous - live`, so `live` has to be the
                // value this frame renders. Without this edge the system is only
                // ordered before the apply, leaving it free to run *before*
                // frame interpolation, which would measure against the raw
                // replayed value instead — and the apply would then put that
                // offset on top of the interpolated value.
                .after(FrameInterpolationSystems::Interpolate)
                .before(RollbackSystems::VisualCorrection),
            // A window's state is cleared as it ends, after its last measurement and before the
            // window goes.
            clear_expired_switch_state::<C, D>
                .in_set(PredictionSystems::All)
                .after(RollbackSystems::VisualCorrection)
                .before(switch::expire_switch_blends),
            // The value a new switch blends from is saved once the request resolves.
            save_switch_visual::<C, D>
                .in_set(PredictionSystems::All)
                .after(switch::save_previous_visuals),
        ),
    );
    app.configure_sets(
        PostUpdate,
        (
            FrameInterpolationSystems::Interpolate,
            // The apply must see the interpolated value: the recorded error is
            // the gap to the value on screen, so it is added to that value.
            RollbackSystems::VisualCorrection,
        )
            .chain(),
    );
    app.add_systems(
        PostUpdate,
        update_visual_correction::<C, D>.in_set(RollbackSystems::VisualCorrection),
    );
    // Correction state cannot outlive its component: without live `C` the
    // apply query never runs, so leftovers would linger until despawn — and a
    // lingering blend window would permanently block every later switch.
    app.add_observer(remove_correction_state_on_live_removed::<C, D>);
    app.add_observer(insert_correction_slots::<C, D>);
}

/// Puts the correction slots on an entity as its prediction history of `C` goes on, so that no
/// rollback or correction later moves it between tables.
fn insert_correction_slots<C: Component, D: Send + Sync + 'static>(
    trigger: On<'_, '_, Add<PredictionHistory<C>>>,
    mut commands: Commands<'_, '_>,
) {
    commands.entity(trigger.entity).try_insert_if_new((
        PreviousVisual::<C>::default(),
        VisualCorrection::<D>::default(),
    ));
}

/// Drops the decaying correction when its live component is removed.
///
/// Without live `C` nothing applies or decays the error, so it would sit there
/// until despawn and be added to whatever the value becomes when the component
/// returns.
///
/// The saved value is deliberately kept: a component can be removed and added
/// back — replication flux, or a rollback that removes it at the rollback tick
/// and re-adds it later in the replay — and the correction for the jump that
/// causes is measured from it. The system that computes corrections decides when
/// a saved value is past saving; see [`crate::correction`]'s creation system.
fn remove_correction_state_on_live_removed<C: Component, D: Send + Sync + 'static>(
    trigger: On<'_, '_, Remove<C>>,
    mut corrections: Query<'_, '_, &mut VisualCorrection<D>>,
) {
    if let Ok(mut correction) = corrections.get_mut(trigger.entity) {
        correction.0 = None;
    }
}

/// Saves the value a timeline switch blends from, for one correction type: the value on screen
/// of each entity whose switch resolved this frame into a blend.
///
/// A switch to the prediction timeline saves nothing: the forced rollback that it asks for
/// captures the same value from the same live component in `PreUpdate` of the next frame, and
/// nothing writes the live value in between, so saving it here would write the same thing twice.
pub(crate) fn save_switch_visual<C, D>(
    switches: Query<'_, '_, (Entity, &PendingSwitch, &C), Added<PendingSwitch>>,
    mut deferred: Deferred<'_, DeferredEntityCommands>,
) where
    C: HistoryComponent + Diffable<D>,
    D: Send + Sync + 'static,
{
    for (entity, switch, value) in &switches {
        if !switch.blends() || switch.direction == SwitchDirection::ToPredicted {
            continue;
        }
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "switch_previous_visual_saved",
            schedule = "PostUpdate",
            sample_point = "PostUpdate",
            entity = ?entity,
            component = ?DebugName::type_name::<C>(),
            value = ?value,
            "saved value for timeline switch blend"
        );
        // An interpolated entity has no slot, as the slots come with the prediction history.
        deferred.insert(entity, PreviousVisual(Some(value.clone())));
    }
}

/// Removes the state a switch kept for one corrected type once its window ends.
///
/// A saved value that is still there when the window ends belongs to a component
/// the destination timeline never presented, and nothing will use it now; a
/// correction still there is converging, and the apply owns it.
pub(crate) fn clear_expired_switch_state<C, D>(
    time: Res<'_, Time<Virtual>>,
    mut blends: Query<'_, '_, (&SwitchBlend, &mut PreviousVisual<C>)>,
) where
    C: HistoryComponent + Diffable<D>,
    D: Send + Sync + 'static,
{
    let now = time.elapsed();
    for (blend, mut previous) in &mut blends {
        if blend.is_expired(now) {
            previous.0 = None;
        }
    }
}

/// Records the correction for the jump a saved value measures, for one
/// corrected type.
///
/// This is the one place a [`VisualCorrection`] is created. Every jump is
/// measured the same way — the value that was on screen before it, against the
/// value that is on screen now — whether the jump came from a rollback or from a
/// timeline switch, so one handler covers both. The only difference between them
/// is how the resulting error is decayed, which the window decides later.
///
/// Runs in `PostUpdate`, after frame interpolation has written the value this
/// frame renders, so the measurement is against exactly what the entity would
/// show without a correction. That is what makes the blend land on the saved
/// value: the apply adds the error to this value, and the error is the gap
/// between it and the saved one.
///
/// `previous` stays in place when the destination timeline has not presented the
/// component yet, so the correction is recorded once it does. Nothing else
/// removes it: a component can be removed and added back, and the jump that
/// causes is measured from the same saved value.
pub(crate) fn create_visual_correction<C, D>(
    time: Res<'_, Time<Virtual>>,
    mut saved: Query<
        '_,
        '_,
        (
            Entity,
            &mut PreviousVisual<C>,
            Option<&C>,
            Option<&mut VisualCorrection<D>>,
            Has<SwitchBlend>,
        ),
    >,
    mut deferred: Deferred<'_, DeferredEntityCommands>,
) where
    C: HistoryComponent + Diffable<D>,
    D: Debug + Send + Sync + 'static,
{
    let start = time.elapsed();
    for (entity, mut previous_visual, live, correction, waiting_for_window) in &mut saved {
        let Some(previous) = &previous_visual.0 else {
            continue;
        };
        let Some(live) = live else {
            // The destination timeline has not presented this component, so
            // there is nothing to measure against yet. A switch window is what
            // is waiting for it, and the window's length bounds the wait;
            // without one nothing will use the saved value, so it goes rather
            // than lingering until despawn and then being added to whatever the
            // value becomes when it returns.
            if !waiting_for_window {
                previous_visual.0 = None;
            }
            continue;
        };
        // `diff(new)` is `new - self`, so this is `previous - live`: the apply
        // adds it to the destination, and the entity keeps rendering the value
        // it had before the jump.
        let error: D = live.diff(previous);
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "visual_correction_created",
            schedule = "PostUpdate",
            sample_point = "PostUpdate",
            entity = ?entity,
            component = ?DebugName::type_name::<C>(),
            previous = ?previous,
            current_visual = ?live,
            error = ?error,
            "created visual correction"
        );
        let new_correction = VisualCorrection(Some(VisualError::<D>::new(error, start)));
        // An entity whose saved value came from a switch, not with a history, may lack the slot.
        if let Some(mut correction) = correction {
            *correction = new_correction;
        } else {
            deferred.insert(entity, new_correction);
        }
        previous_visual.0 = None;
    }
}

/// Applies and decays a stored visual correction after frame interpolation.
///
/// This typed system runs in [`PostUpdate`], in
/// [`RollbackSystems::VisualCorrection`], after
/// [`FrameInterpolationSystems::Interpolate`]. Frame interpolation first writes
/// the corrected visual value for the render frame; this system then applies
/// the decaying [`VisualCorrection`] error on top. If the remaining error is
/// small enough, it removes the correction component.
///
/// `C` must have an interpolation rule with a frame-interpolation apply
/// function, because the error is measured against the value that rule produced
/// for this frame. The error is stored as `D` and given up by the correction
/// function registered through `correct_linear` or `correct_with`.
pub(crate) fn update_visual_correction<
    C: HistoryComponent + Diffable<D>,
    D: Default + Clone + Debug + Send + Sync + 'static,
>(
    time: Res<'_, Time<Virtual>>,
    correction: Res<'_, CorrectionFn<C, D>>,
    manager: Res<'_, PredictionManager>,
    mut query: Query<
        '_,
        '_,
        (
            Entity,
            &mut C,
            &mut VisualCorrection<D>,
            Option<&CorrectionPolicy>,
            Option<&SwitchBlend>,
        ),
    >,
) {
    let dt = time.delta();
    let now = time.elapsed();
    let global = &manager.correction_policy;
    query.iter_mut().for_each(
        |(entity, mut component, mut slot, override_policy, blend_marker)| {
            // An idle slot is only read, so its change tick stays.
            if slot.0.is_none() {
                return;
            }
            let Some(visual_correction) = slot.0.as_mut() else {
                return;
            };
            let dt_secs = dt.as_secs_f32();
            let elapsed_secs = visual_correction.elapsed_secs(now);
            let policy_ratio = override_policy
                .map_or(global, |policy| policy)
                .lerp_ratio(elapsed_secs, dt);
            // A live window's ease curve shapes the decay, so the length and
            // curve the caller asked for are what the transition looks like. The
            // curve never gives the error up *faster* than the entity's own
            // correction would: its tail is steep, and a jump arriving late in
            // the window (a rollback during the blend) would otherwise be dumped
            // into a single frame. The policy is the floor, the curve the
            // ceiling, and a converged error ends the blend early.
            let r = blend_marker
                .filter(|marker| !marker.is_expired(now))
                .and_then(|marker| marker.curve_keep(now, dt_secs))
                .map_or(policy_ratio, |curve_ratio| curve_ratio.max(policy_ratio));
            // The correction converges on its own rule, whatever the error's type: once it keeps
            // under `CONVERGED` of its recorded jump, or its window ends, nothing is left to carry
            // and the next jump will record a fresh one.
            let remaining = visual_correction.remaining * r;
            if remaining <= CONVERGED {
                trace!(
                    target: "campfire_net_debug::prediction",
                    kind = "visual_correction_removed",
                    schedule = "PostUpdate",
                    sample_point = "PostUpdate",
                    entity = ?entity,
                    component = ?DebugName::type_name::<C>(),
                    error = ?visual_correction.error,
                    "removed visual correction because it converged"
                );
                slot.0 = None;
                return;
            }
            visual_correction.remaining = remaining;
            let new_error = (correction.correction_fn)(&D::default(), &visual_correction.error, r);
            component.apply_diff(&new_error);
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "visual_correction_apply",
                schedule = "PostUpdate",
                sample_point = "PostUpdate",
                entity = ?entity,
                component = ?DebugName::type_name::<C>(),
                previous_error = ?visual_correction.error,
                new_error = ?new_error,
                ratio = r,
                "applied visual correction"
            );
            visual_correction.error = new_error;
        },
    );
}

#[cfg(test)]
mod tests;
