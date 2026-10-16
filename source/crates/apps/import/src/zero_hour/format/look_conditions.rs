/// The conditions of a map that a model's state follows, as the original sets them on every
/// object with `ForceModelsToFollowTimeOfDay` and `ForceModelsToFollowWeather`: `NIGHT` on a night
/// map and `SNOW` on a snowy one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LookConditions {
    pub(crate) night: bool,
    pub(crate) snow: bool,
}

impl LookConditions {
    /// The conditions a look writes a variant for: a night map's, a snowy map's, and both.
    pub(crate) const VARIANTS: [Self; 3] = [
        Self {
            night: true,
            snow: false,
        },
        Self {
            night: false,
            snow: true,
        },
        Self {
            night: true,
            snow: true,
        },
    ];
}
