//! Client-side visual correction for predicted components.
//!
//! Prediction rollback has two separate goals:
//! - the simulation must immediately use the corrected state produced by
//!   rollback and replay;
//! - the rendered value should not snap from the pre-rollback visual state to
//!   the corrected visual state in one frame.
//!
//! A predicted component `C` shows through a visual component `V`, which a frame rule writes each
//! frame from `C`'s last two ticks while `C` keeps the simulated value (11-net-stack/replication.md,
//! The value on screen is its own component). Correction is installed for components registered
//! with `correct_linear`, `correct_with`, or `correct_custom`. A built-in correction adds typed
//! systems for `C`, its visual `V` and the error `D` of `V`, which record, decay and clear its
//! [`VisualCorrection`] on `V`.
//!
//! # How a correction is recorded
//!
//! Every correction is one measurement of one jump: the visual before it, against the visual
//! now.
//!
//! The visual before the jump is saved as [`PreviousVisual`], and the visual now is whatever the
//! frame wrote into `V` when the correction is recorded. Recording happens in `PostUpdate`, after
//! [`FrameInterpolationSystems::Interpolate`] has written the value the frame renders, so the
//! measurement is against exactly what the entity would show without a correction.
//!
//! So a correction samples nothing of its own: it runs after the frame interpolation it would
//! otherwise repeat. A timeline switch is the same measurement with a different value before the
//! jump (see [`crate::timeline_switch_plugin`]), so it goes through this same system.
//!
//! # How the error is given up
//!
//! [`VisualCorrection`] holds the error and the clock reading it was recorded at.
//! `CorrectionFn::update_visual_correction` multiplies the error by a keep ratio each frame:
//!
//! - on the entity's [`CorrectionPolicy`], or the global one on [`PredictionSetup`], which defaults
//!   to an unbounded exponential and can be set to any
//!   [`CorrectionEase`](crate::correction_ease::CorrectionEase) with its own duration;
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
//! - [`FrameInterpolationSystems::Interpolate`] runs in `PostUpdate` and writes
//!   `V` from those two values at the current fixed overstep, on each entity whose
//!   archetype a frame rule of `C` writes; only there does the offset leave `V` with
//!   the next frame's write, so only there is it applied.
//! - Rollback runs in `PreUpdate`. As it prepares, it saves [`PreviousVisual`] from `V`,
//!   which still holds what the last frame showed. Replay then advances `C` to the
//!   corrected simulation value for the current tick, and rewrites the prediction
//!   history the frame interpolation reads, so nothing repairs a frame history after it.
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
use crate::prediction_registry::PredictionRegistry;
use crate::prediction_setup::PredictionSetup;
use crate::prediction_systems::PredictionSystems;
use crate::previous_visual::PreviousVisual;
use crate::rollback_systems::RollbackSystems;
use crate::switch_blend::SwitchBlend;
use crate::switch_direction::SwitchDirection;
use crate::timeline_switch_plugin::TimelineSwitchPlugin;
use crate::visual_correction::{CONVERGED, VisualCorrection, VisualOffset};
use bevy_app::prelude::*;
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_time::{Time, Virtual};
use bevy_utils::prelude::DebugName;
use campfire_net_frame_interpolation::FrameHolders;
use campfire_net_frame_interpolation::FrameInterpolationSystems;
use campfire_net_frame_interpolation::FrameRegistry;
use campfire_net_frame_interpolation::LerpFn;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::Diffable;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::Interpolated;
use core::fmt::Debug;
use tracing::trace;

/// The correction fn of component `C`'s visual `V` on errors of type `D`, which the prediction
/// registry holds and its `(C, V, D)` correction systems read typed.
pub(crate) struct CorrectionFn<C, V, D> {
    correction_fn: LerpFn<D>,
    marker: core::marker::PhantomData<fn() -> (C, V)>,
}

impl<C, V, D> CorrectionFn<C, V, D> {
    pub(crate) const fn new(correction_fn: LerpFn<D>) -> Self {
        Self {
            correction_fn,
            marker: core::marker::PhantomData,
        }
    }

    /// Saves the value a timeline switch blends from, for one correction type: the value on screen
    /// of each entity whose switch resolved this frame into a blend.
    ///
    /// A switch to the prediction timeline saves nothing: the forced rollback that it asks for
    /// saves the same visual in `PreUpdate` of the next frame, and no frame writes it in between,
    /// so saving it here would write the same thing twice.
    pub(crate) fn save_switch_visual(
        switches: Query<'_, '_, (Entity, &PendingSwitch, &V), Added<PendingSwitch>>,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) where
        V: HistoryComponent + Diffable<D>,
        D: Send + Sync + 'static,
    {
        for (entity, switch, value) in &switches {
            if !switch.blends() || switch.direction == SwitchDirection::ToPredicted {
                continue;
            }
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "switch_previous_visual_saved",
                entity = ?entity,
                component = ?DebugName::type_name::<V>(),
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
    /// correction still there is converging, and the apply holds it.
    pub(crate) fn clear_expired_switch_state(
        time: Res<'_, Time<Virtual>>,
        mut blends: Query<'_, '_, (&SwitchBlend, &mut PreviousVisual<V>)>,
    ) where
        V: HistoryComponent + Diffable<D>,
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
    pub(crate) fn create_visual_correction(
        time: Res<'_, Time<Virtual>>,
        mut saved: Query<
            '_,
            '_,
            (
                Entity,
                &mut PreviousVisual<V>,
                Option<&V>,
                Option<&mut VisualCorrection<D>>,
                Has<SwitchBlend>,
            ),
        >,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) where
        V: HistoryComponent + Diffable<D>,
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
                entity = ?entity,
                component = ?DebugName::type_name::<V>(),
                previous = ?previous,
                current_visual = ?live,
                error = ?error,
                "created visual correction"
            );
            let new_correction = VisualCorrection(Some(VisualOffset::<D>::new(error, start)));
            // An entity whose saved value came from a switch, not with a history, may lack the
            // slot.
            if let Some(mut correction) = correction {
                *correction = new_correction;
            } else {
                deferred.insert(entity, new_correction);
            }
            previous_visual.0 = None;
        }
    }
}

impl<C, V, D> Debug for CorrectionFn<C, V, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CorrectionFn").finish_non_exhaustive()
    }
}

/// Installs built-in visual correction systems for predicted component `C`, on its visual `V`.
///
/// The visual is saved as a rollback prepares, the corrections are recorded in [`PostUpdate`],
/// after frame interpolation, and the visual error decays in [`PostUpdate`], in
/// [`RollbackSystems::VisualCorrection`].
pub(crate) fn add_correction_systems<
    C: HistoryComponent,
    V: HistoryComponent + Diffable<D>,
    D: Default + Clone + Debug + Send + Sync + 'static,
>(
    app: &mut App,
) {
    // A correction offsets the visual a frame writes, which the next frame writes again; a visual
    // no frame rule of `C` writes would keep each offset. The registry sealed before prediction's.
    let shown = app
        .world()
        .get_resource::<FrameRegistry>()
        .is_some_and(FrameRegistry::maps::<C, V>);
    assert!(
        shown,
        "{} corrects rollbacks on {}, which no frame rule shows it through",
        core::any::type_name::<C>(),
        core::any::type_name::<V>()
    );
    app.add_systems(
        PreUpdate,
        CorrectionFn::<C, V, D>::save_rollback_visual.in_set(RollbackSystems::Prepare),
    );
    app.add_systems(
        PostUpdate,
        (
            CorrectionFn::<C, V, D>::create_visual_correction
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
            CorrectionFn::<C, V, D>::clear_expired_switch_state
                .in_set(PredictionSystems::All)
                .after(RollbackSystems::VisualCorrection)
                .before(TimelineSwitchPlugin::expire_switch_blends),
            // The value a new switch blends from is saved once the request resolves.
            CorrectionFn::<C, V, D>::save_switch_visual
                .in_set(PredictionSystems::All)
                .after(TimelineSwitchPlugin::save_previous_visuals),
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
        CorrectionFn::<C, V, D>::update_visual_correction.in_set(RollbackSystems::VisualCorrection),
    );
    // Correction state cannot outlive its component: without live `C` the
    // apply query never runs, so leftovers would linger until despawn — and a
    // lingering blend window would permanently block every later switch.
    app.add_observer(CorrectionFn::<C, V, D>::remove_correction_state_on_live_removed);
    app.add_observer(CorrectionFn::<C, V, D>::insert_correction_slots);
}

impl<C, V, D> CorrectionFn<C, V, D>
where
    C: HistoryComponent,
    V: HistoryComponent + Diffable<D>,
    D: Default + Clone + Debug + Send + Sync + 'static,
{
    /// Saves the visual each predicted entity shows as a rollback prepares: the rollback rewrites
    /// `C`, never `V`, which still holds what the last frame showed, the value a correction is
    /// measured from. An interpolated entity is not corrected, as its visual follows the delayed
    /// interpolation.
    pub(crate) fn save_rollback_visual(
        mut visuals: Query<
            '_,
            '_,
            (Entity, &V, Option<&mut PreviousVisual<V>>),
            (With<PredictionHistory<C>>, Without<Interpolated>),
        >,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) {
        for (entity, visual, previous) in &mut visuals {
            let saved = PreviousVisual(Some(visual.clone()));
            // The slot comes with the history, so the write moves no table; one an app put the
            // history on by hand takes it now.
            match previous {
                Some(mut previous) => *previous = saved,
                None => deferred.insert(entity, saved),
            }
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "previous_visual_stored",
                entity = ?entity,
                component = ?DebugName::type_name::<V>(),
                "stored previous visual for correction"
            );
        }
    }

    /// Applies and decays a stored visual correction after frame interpolation.
    ///
    /// This typed system runs in [`PostUpdate`], in [`RollbackSystems::VisualCorrection`], after
    /// [`FrameInterpolationSystems::Interpolate`]. Frame interpolation first writes the visual for
    /// the render frame; this system then applies the decaying [`VisualCorrection`] error on top,
    /// on each entity whose visual a frame rule of `C` wrote, as only that write takes the offset
    /// away again. If the remaining error is small enough, it removes the correction.
    ///
    /// The error is stored as `D` and given up by the correction function registered through
    /// `correct_linear` or `correct_with`.
    pub(crate) fn update_visual_correction(
        time: Res<'_, Time<Virtual>>,
        registry: Res<'_, PredictionRegistry>,
        setup: Res<'_, PredictionSetup>,
        holders: Res<'_, FrameHolders>,
        mut query: Query<
            '_,
            '_,
            (
                Entity,
                EntityLocation,
                &mut V,
                &mut VisualCorrection<D>,
                Option<&CorrectionPolicy>,
                Option<&SwitchBlend>,
            ),
        >,
    ) {
        let correction_fn = registry.stack_correction::<C, Self>().correction_fn;
        let dt = time.delta();
        let now = time.elapsed();
        let global = &setup.correction_policy;
        query.iter_mut().for_each(
            |(entity, location, mut visual, mut slot, override_policy, blend_marker)| {
                // An idle slot is only read, so its change tick stays.
                if slot.0.is_none() || !holders.shows::<C>(location.archetype_id) {
                    return;
                }
                let Some(visual_correction) = slot.0.as_mut() else {
                    return;
                };
                let elapsed = visual_correction.elapsed(now);
                let policy_ratio = override_policy
                    .map_or(global, |policy| policy)
                    .lerp_ratio(elapsed, dt);
                // A live window's ease curve shapes the decay, so the length and
                // curve the caller asked for are what the transition looks like. The
                // curve never gives the error up *faster* than the entity's own
                // correction would: its tail is steep, and a jump arriving late in
                // the window (a rollback during the blend) would otherwise be dumped
                // into a single frame. The policy is the floor, the curve the
                // ceiling, and a converged error ends the blend early.
                let r = blend_marker
                    .filter(|marker| !marker.is_expired(now))
                    .and_then(|marker| marker.curve_keep(now, dt))
                    .map_or(policy_ratio, |curve_ratio| curve_ratio.max(policy_ratio));
                // The correction converges on its own rule, whatever the error's type: once it
                // keeps under `CONVERGED` of its recorded jump, or its window ends, nothing is left
                // to carry and the next jump will record a fresh one.
                let remaining = visual_correction.remaining * r;
                if remaining <= CONVERGED {
                    trace!(
                        target: "campfire_net_debug::prediction",
                        kind = "visual_correction_removed",
                        entity = ?entity,
                        component = ?DebugName::type_name::<V>(),
                        error = ?visual_correction.error,
                        "removed visual correction because it converged"
                    );
                    slot.0 = None;
                    return;
                }
                visual_correction.remaining = remaining;
                let new_error = correction_fn(&D::default(), &visual_correction.error, r);
                visual.apply_diff(&new_error);
                trace!(
                    target: "campfire_net_debug::prediction",
                    kind = "visual_correction_apply",
                    entity = ?entity,
                    component = ?DebugName::type_name::<V>(),
                    previous_error = ?visual_correction.error,
                    new_error = ?new_error,
                    ratio = r,
                    "applied visual correction"
                );
                visual_correction.error = new_error;
            },
        );
    }
}

impl<C: Component, V: Component, D: Send + Sync + 'static> CorrectionFn<C, V, D> {
    /// Puts the correction slots on an entity as its prediction history of `C` goes on, so that no
    /// rollback or correction later moves it between tables.
    fn insert_correction_slots(
        trigger: On<'_, '_, Add<PredictionHistory<C>>>,
        mut commands: Commands<'_, '_>,
    ) {
        commands.entity(trigger.entity).try_insert_if_new((
            PreviousVisual::<V>::default(),
            VisualCorrection::<D>::default(),
        ));
    }

    /// Drops the decaying correction when its live component is removed.
    ///
    /// Without live `C` no frame rule writes `V`, so nothing applies or decays the error, which
    /// would sit there until despawn and be added to whatever the visual becomes when the
    /// component returns.
    ///
    /// The saved value is deliberately kept: a component can be removed and added
    /// back — replication flux, or a rollback that removes it at the rollback tick
    /// and re-adds it later in the replay — and the correction for the jump that
    /// causes is measured from it. The system that computes corrections decides when
    /// a saved value is past saving; see [`crate::correction`]'s creation system.
    fn remove_correction_state_on_live_removed(
        trigger: On<'_, '_, Remove<C>>,
        mut corrections: Query<'_, '_, &mut VisualCorrection<D>>,
    ) {
        if let Ok(mut correction) = corrections.get_mut(trigger.entity) {
            correction.0 = None;
        }
    }
}

#[cfg(test)]
mod tests;
