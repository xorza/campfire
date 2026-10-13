use campfire_common::Tick;

/// When a logged input takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    At(Tick),
    /// It arrived more than the max input delay after its stamp. It stays in the chain but never
    /// takes effect.
    Late,
    /// Its stamp was more than the max input lead ahead of the next tick. Like a late input, it
    /// stays in the chain but never takes effect.
    Early,
}
