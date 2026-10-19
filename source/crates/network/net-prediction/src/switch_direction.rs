/// Direction of a `TimelineSwitch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchDirection {
    /// Move the entity to the prediction timeline.
    ToPredicted,
    /// Move the entity to the interpolation timeline.
    ToInterpolated,
}
