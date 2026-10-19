use crate::interpolation_sample_context::InterpolationSampleContext;
use bevy_ecs::component::Component;
use core::any::Any;
use core::fmt;
use core::marker::PhantomData;
use std::sync::Arc;

/// Function used to interpolate from one component state (`start`) to another (`other`):
/// `t` goes from 0.0 (`start`) to 1.0 (`other`).
pub type LerpFn<C> = fn(start: &C, other: &C, t: f32) -> C;

/// The interpolation of a component rule, as the rule stores it.
pub(crate) type ComponentInterpolation<C> =
    dyn Fn(&C, &C, InterpolationSampleContext) -> C + Send + Sync;

/// Interpolation function stored by a rule: a component's, which reads each endpoint as `&C`, or
/// a bundle's, which reads it as a tuple of references to the members. A fn of a fraction alone
/// is stored as one that reads it from the sample's context.
pub struct InterpolationFn<S> {
    /// The boxed fn of the rule's shape.
    erased: Arc<dyn Any + Send + Sync>,
    marker: PhantomData<fn() -> S>,
}

impl<S> InterpolationFn<S> {
    /// `interpolation`, a boxed fn of the shape of `S`'s rule.
    pub(crate) fn erase<F: ?Sized + Send + Sync + 'static>(interpolation: Box<F>) -> Self {
        Self {
            erased: Arc::new(interpolation),
            marker: PhantomData,
        }
    }

    /// The boxed fn, of the shape `F` of `S`'s rule.
    ///
    /// # Panics
    ///
    /// When `F` is not the shape the fn was erased with.
    pub(crate) fn typed<F: ?Sized + 'static>(&self) -> &F {
        self.erased
            .downcast_ref::<Box<F>>()
            .expect("a rule's interpolation fn has the shape of its rule")
    }
}

impl<S> Clone for InterpolationFn<S> {
    fn clone(&self) -> Self {
        Self {
            erased: Arc::clone(&self.erased),
            marker: PhantomData,
        }
    }
}

impl<S> fmt::Debug for InterpolationFn<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InterpolationFn(..)")
    }
}

impl<C: Component> InterpolationFn<C> {
    /// Applies the interpolation function using the provided context.
    pub fn interpolate(&self, start: &C, other: &C, context: InterpolationSampleContext) -> C {
        self.component()(start, other, context)
    }

    /// The fn of a component rule, which its system takes once for each run.
    pub(crate) fn component(&self) -> &ComponentInterpolation<C> {
        self.typed::<ComponentInterpolation<C>>()
    }
}
