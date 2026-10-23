use campfire_common::NamedEnum;

/// A field of an action's data that the pipeline runs for every kind that takes it, beside the
/// name and the targeting every action gives, and the kind's own section: each key of
/// `ActionKeys` but those two, its delivery and its lists, by the name data writes it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub enum PipelineField {
    Script,
    Range,
    MinRange,
    CooldownMs,
    Cost,
    WindupMs,
    ClampToRange,
    Toggle,
    Channel,
    Hold,
    Charges,
    Charging,
    PassiveModifier,
    PassiveWhileReady,
    Delivery,
    Params,
    OnResolve,
    OnHit,
    OnEnd,
}

impl PipelineField {
    /// Whether another field's take decides it: a minimum range a range's, a passive's hold while
    /// ready its passive's.
    pub(crate) const fn follows(self) -> bool {
        matches!(self, Self::MinRange | Self::PassiveWhileReady)
    }
}
