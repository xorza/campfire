//! The constructors of a rule's interpolation fn, one for each shape of fn, apart by a marker type
//! as Bevy's `IntoSystem<In, Out, Marker>` keeps its fn shapes apart.

use crate::interpolation_fn::{ComponentInterpolation, InterpolationFn};
use crate::interpolation_sample_cx::InterpolationSampleCx;
use bevy_ecs::component::Component;

/// A fn of a fraction that interpolates the rule's target `S`: `Fn(&C, &C, f32) -> C` for a
/// component, `Fn((&A, &B), (&A, &B), f32) -> (A, B)` for a bundle (11-net-stack/api.md, A rule's
/// fn goes through one generic constructor for each shape).
///
/// `M` tells the shapes apart, so a fn item, a fn pointer or a closure with typed parameters
/// infers it; a closure whose parameters are untyped needs their types.
pub trait IntoLerpFn<S, M>: Send + Sync + 'static {
    fn into_interpolation_fn(self) -> InterpolationFn<S>;
}

/// A fn of the sample's context that interpolates the rule's target `S`:
/// `Fn(&C, &C, InterpolationSampleCx) -> C` for a component, with a tuple of references for
/// a bundle, as [`IntoLerpFn`] takes them.
pub trait IntoCxLerpFn<S, M>: Send + Sync + 'static {
    fn into_interpolation_fn(self) -> InterpolationFn<S>;
}

/// The marker of a component's fn shape.
#[derive(Debug)]
pub struct ComponentShape;

impl<C: Component, F> IntoLerpFn<C, ComponentShape> for F
where
    F: Fn(&C, &C, f32) -> C + Send + Sync + 'static,
{
    fn into_interpolation_fn(self) -> InterpolationFn<C> {
        let interpolation: Box<ComponentInterpolation<C>> =
            Box::new(move |start: &C, other: &C, cx: InterpolationSampleCx| {
                self(start, other, cx.t)
            });
        InterpolationFn::erase(interpolation)
    }
}

impl<C: Component, F> IntoCxLerpFn<C, ComponentShape> for F
where
    F: Fn(&C, &C, InterpolationSampleCx) -> C + Send + Sync + 'static,
{
    fn into_interpolation_fn(self) -> InterpolationFn<C> {
        let interpolation: Box<ComponentInterpolation<C>> = Box::new(self);
        InterpolationFn::erase(interpolation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpolation_fns::InterpolationFns;

    #[derive(bevy_ecs::component::Component, Clone, Debug, PartialEq)]
    struct A(f32);

    #[derive(bevy_ecs::component::Component, Clone, Debug, PartialEq)]
    struct B(f32);

    type Members = dyn Fn((&A, &B), (&A, &B), InterpolationSampleCx) -> (A, B) + Send + Sync;

    fn lerp(start: &A, end: &A, t: f32) -> A {
        A(start.0 + (end.0 - start.0) * t)
    }

    fn pair(start: (&A, &B), end: (&A, &B), t: f32) -> (A, B) {
        (
            lerp(start.0, end.0, t),
            B(start.1.0 + (end.1.0 - start.1.0) * t),
        )
    }

    fn cx_pair(_: (&A, &B), _: (&A, &B), cx: InterpolationSampleCx) -> (A, B) {
        (A(cx.t), B(cx.sample_delta_secs.unwrap()))
    }

    /// Each shape infers its target from the fn: a component's fn at a quarter from 0 to 4 gives
    /// 1, a bundle's fn reads its members by reference and gives (1, 2) from (0, 0) to (4, 8), and
    /// a fn of the context reads its fraction and its interval.
    #[test]
    fn each_fn_shape_infers_its_target() {
        let cx = InterpolationSampleCx::new(0.25, Some(0.5));

        let component = InterpolationFns::with(lerp).interpolation.unwrap();
        assert_eq!(component.interpolate(&A(0.0), &A(4.0), cx), A(1.0));

        let bundle = InterpolationFns::with(pair).interpolation.unwrap();
        assert_eq!(
            bundle.typed::<Members>()((&A(0.0), &B(0.0)), (&A(4.0), &B(8.0)), cx),
            (A(1.0), B(2.0))
        );
        let contextual = InterpolationFns::with_cx(cx_pair).interpolation.unwrap();
        assert_eq!(
            contextual.typed::<Members>()((&A(0.0), &B(0.0)), (&A(4.0), &B(8.0)), cx),
            (A(0.25), B(0.5))
        );
    }
}
