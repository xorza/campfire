//! The one crate of the workspace whose code differs by OS (02-engine-core.md, Platform): each
//! entity here keeps one promise on every OS behind one interface, and `os`, one file for each OS
//! family, keeps it the way its OS can.

#![expect(
    clippy::disallowed_types,
    reason = "the OS's files, which clippy.toml denies the crates above the platform"
)]
// Windows names and exposes files by its own calls, so Unix alone makes the banned ones.
#![cfg_attr(
    unix,
    expect(
        clippy::disallowed_methods,
        reason = "the OS's file calls, which clippy.toml denies the crates above the platform"
    )
)]

mod durable_name;
#[cfg(feature = "internals")]
mod file_link;
mod no_follow;
mod positional_read;
mod shared_port;
mod stop_signals;
mod user_only;

// An OS of neither family names no file here, so the build fails where no promise is kept.
#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

pub use crate::durable_name::DurableName;
pub use crate::no_follow::NoFollow;
pub use crate::positional_read::PositionalRead;
pub use crate::shared_port::SharedPort;
pub use crate::stop_signals::StopSignals;
pub use crate::user_only::{Exposure, UserOnly};

/// What the tests of the engine's crates use of the platform: a link.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::file_link::FileLink;
}
