use campfire_common::Tick;

/// When a logged input takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    At(Tick),
    /// Its stamp was more than the max input lead ahead of the next tick. It stays in the chain
    /// but never takes effect.
    Early,
}
