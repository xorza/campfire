use crate::correction;
use crate::mismatch_fn::MismatchFn;
use crate::prediction_registration;
use crate::prediction_registry;
use bevy_curve::Ease;
use campfire_net_interpolation::HistoryComponent;
use campfire_net_interpolation::LerpFn;
use campfire_net_interpolation::linear_lerp;
use campfire_net_replication::ComponentRegistration;
use campfire_net_replication::ComponentRegistrator;
use campfire_net_replication::Diffable;
use core::fmt::Debug;

/// Registration state returned after prediction has been enabled for a component.
///
/// Rollback comparison and visual correction are only meaningful for a predicted component, so
/// they are only on this state.
///
/// A correction smooths a rollback by storing the pre-rollback visual value and then decaying
/// the difference between that value and the corrected state over several frames. It does not
/// register an interpolation rule: the corrected component needs an applicable rule with an
/// interpolation function when correction runs, a component rule for `C` or a bundle rule such
/// as `(A, B)` that contains `C`. Other members of the selected bundle need no correction of
/// their own: their repaired predicted frame histories are still available as inputs when
/// computing the corrected sample for `C`.
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

    /// Store the pre-rollback visual value without the stack's correction systems, for a
    /// correction applied elsewhere: for example when `Position` and `Rotation` are predicted
    /// but correction and frame interpolation are applied on `Transform`.
    pub fn correct_custom(self) -> Self
    where
        C: HistoryComponent,
    {
        prediction_registry::open(self.registration.app).set_corrected::<C>();
        self
    }

    /// Correct rollbacks by linear interpolation on errors of type `D`, the diff type
    /// [`Diffable::diff`] returns for `C`: the error decays from `D::default()` along an
    /// [`Ease`] curve. `D` is `C` for a component that diffs into itself.
    pub fn correct_linear<D>(self) -> Self
    where
        C: HistoryComponent + Diffable<D>,
        D: Ease + Debug + Clone + Default + Send + Sync + 'static,
    {
        self.correct_with::<D>(linear_lerp::<D>)
    }

    /// Correct rollbacks by `correction_fn` on errors of type `D`: for a `D` that should not
    /// use [`Ease`] interpolation, or whose decay follows component-specific logic.
    pub fn correct_with<D>(self, correction_fn: LerpFn<D>) -> Self
    where
        C: HistoryComponent + Diffable<D>,
        D: Debug + Clone + Default + Send + Sync + 'static,
    {
        let app = &mut *self.registration.app;
        app.insert_resource(correction::CorrectionFn::<C, D>::new(correction_fn));
        let mut registry = prediction_registry::open(app);
        registry.set_corrected::<C>();
        registry.defer(correction::add_correction_systems::<C, D>);
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
