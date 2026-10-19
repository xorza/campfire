//! Interpolation fns for common types, which a rule can name as its lerp.
use bevy_curve::{Curve, Ease, EaseFunction, EasingCurve};

/// The linear interpolation from `start` to `other` along their [`Ease`] curve.
pub fn linear_lerp<C>(start: &C, other: &C, t: f32) -> C
where
    C: Ease + Clone,
{
    let curve = EasingCurve::new(start.clone(), other.clone(), EaseFunction::Linear);
    curve.sample_unchecked(t)
}
