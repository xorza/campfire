//! How a Bevy app of the engine uses threads: an app of one thread, whose task pools hold one
//! thread each and whose schedules each run on the thread that updates it (02-engine-core.md, An
//! app's threads are one plan). A proxy crate: the task pools' plugin and options, which
//! `clippy.toml` denies every other crate, are named here alone.

#![expect(
    clippy::disallowed_types,
    reason = "the one crate that sizes Bevy's task pools, which clippy.toml denies the others"
)]

mod one_thread_plugins;

pub use crate::one_thread_plugins::OneThreadPlugins;
