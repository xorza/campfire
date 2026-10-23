use crate::correction;
use crate::mismatch_fn::MismatchFn;
use crate::prediction_registration;
use crate::prediction_registry;
use bevy_curve::Ease;
use campfire_net_frame_interpolation::LerpFn;
use campfire_net_frame_interpolation::linear_lerp;
use campfire_net_replication::ComponentRegistration;
use campfire_net_replication::ComponentRegistrator;
use campfire_net_replication::Diffable;
use campfire_net_replication::HistoryComponent;
use core::fmt::Debug;

/// Registration state returned after prediction has been enabled for a component.
///
/// Rollback comparison and visual correction are only meaningful for a predicted component, so
/// they are only on this state.
///
/// A correction smooths a rollback by storing the visual before it and then decaying the
/// difference between that visual and the one the corrected state shows over several frames. It
/// registers no frame rule: the corrected component needs a frame rule that shows it through the
/// visual the correction names.
#[derive(Debug)]
pub struct PredictedComponentRegistration<'a, C, M> {
    registration: ComponentRegistration<'a, C, M>,
}

#[expect(
    clippy::return_self_not_must_use,
    reason = "the call registers on the app; the returned registration only chains further calls"
)]
impl<'a, C, M> PredictedComponentRegistration<'a, C, M> {
    pub(crate) const fn new(registration: ComponentRegistration<'a, C, M>) -> Self {
        Self { registration }
    }

    /// Compare the confirmed component with the predicted component's history through `mismatch`
    /// instead of [`PartialEq::ne`]: a mismatch triggers a rollback.
    pub fn mismatch_with(self, mismatch: MismatchFn<C>) -> Self
    where
        C: HistoryComponent,
    {
        prediction_registration::set_mismatch(self.registration.app, mismatch);
        self
    }

    /// Store the value `C` held before each rollback without the stack's correction systems, for
    /// a correction the app applies itself: for example when `Position` and `Rotation` are
    /// predicted and the app's own system corrects the `Transform` it makes of both.
    pub fn correct_custom(self) -> Self
    where
        C: HistoryComponent,
    {
        prediction_registry::open(self.registration.app).set_custom_correction::<C>();
        self
    }

    /// Correct rollbacks on `C`'s visual `V` by linear interpolation on errors of type `D`, the
    /// diff type [`Diffable::diff`] returns for `V`: the error decays from `D::default()` along an
    /// [`Ease`] curve. `D` is `V` for a visual that diffs into itself.
    pub fn correct_linear<V, D>(self) -> Self
    where
        C: HistoryComponent,
        V: HistoryComponent + Diffable<D>,
        D: Ease + Debug + Clone + Default + Send + Sync + 'static,
    {
        self.correct_with::<V, D>(linear_lerp::<D>)
    }

    /// Correct rollbacks on `C`'s visual `V` by `correction_fn` on errors of type `D`: for a `D`
    /// that should not use [`Ease`] interpolation, or whose decay follows component-specific
    /// logic.
    ///
    /// # Panics
    ///
    /// As the app finishes its plugins, when no frame rule shows `C` through `V`.
    pub fn correct_with<V, D>(self, correction_fn: LerpFn<D>) -> Self
    where
        C: HistoryComponent,
        V: HistoryComponent + Diffable<D>,
        D: Debug + Clone + Default + Send + Sync + 'static,
    {
        let app = &mut *self.registration.app;
        let mut registry = prediction_registry::open(app);
        let correction = correction::CorrectionFn::<C, V, D>::new(correction_fn);
        registry.set_stack_correction::<C>(correction);
        registry.defer(correction::add_correction_systems::<C, V, D>);
        self
    }
}

impl<'a, C, M> ComponentRegistrator<'a, C, M> for PredictedComponentRegistration<'a, C, M> {
    fn into_component_registration(self) -> ComponentRegistration<'a, C, M> {
        self.registration
    }

    fn from_component_registration(registration: ComponentRegistration<'a, C, M>) -> Self {
        Self::new(registration)
    }
}
