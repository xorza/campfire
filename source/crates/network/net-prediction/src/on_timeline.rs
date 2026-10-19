/// The timeline an entity is on, as its `Predicted` and `Interpolated` markers say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnTimeline {
    Neither,
    Predicted,
    Interpolated,
}

impl OnTimeline {
    /// The timeline of an entity with the markers it has: prediction, when it has both, as the
    /// sender sends only `Predicted` to a client both targets name.
    pub(crate) const fn of(predicted: bool, interpolated: bool) -> Self {
        match (predicted, interpolated) {
            (true, _) => Self::Predicted,
            (false, true) => Self::Interpolated,
            (false, false) => Self::Neither,
        }
    }
}
