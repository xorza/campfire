//! Channels add delivery and ordering policies on top of the packet transport.

/// A channel: a type that names one set of [`ChannelConfig`](crate::channel_config::ChannelConfig).
///
/// Implement it on a marker type, `impl Channel for MyChannel {}`, so that no other type can be
/// used where a channel is expected.
pub trait Channel: Send + Sync + 'static {}
