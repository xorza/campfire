//! How a registered component replicates, as the type parameter of its
//! `ComponentRegistration`: prediction
//! and interpolation read their mode from it.

/// A component registered without a replication rule.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotReplicated;

/// A component that replicates whole, by Replicon's default rule or a custom one.
#[derive(Debug, Clone, Copy, Default)]
pub struct FullReplicated;

/// A component that replicates as diffs, by Replicon's diff rule.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffReplicated;

/// How a registered component replicates, as the registry records it for a layer that reads it
/// at run time: the value of [`FullReplicated`] or [`DiffReplicated`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationKind {
    Full,
    Diff,
}
