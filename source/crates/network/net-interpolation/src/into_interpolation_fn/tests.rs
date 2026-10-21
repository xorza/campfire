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
