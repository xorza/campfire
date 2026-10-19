use serde::Deserialize;

/// What a modifier's interval does as its carrier comes back to life, its hook having run
/// nothing while the carrier was dead (04-capabilities/stats.md, An interval's policy at a
/// respawn): the three policies Unreal's `EGameplayEffectPeriodInhibitionRemovedPolicy` names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntervalRevival {
    /// The interval kept its clock while the carrier was dead, and comes when it would have.
    #[default]
    Continue,
    /// The interval comes one whole interval after the return.
    Reset,
    /// The interval comes at the return, then one whole interval on.
    FireAndReset,
}
