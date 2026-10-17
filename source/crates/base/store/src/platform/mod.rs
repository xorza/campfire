//! The one place of the workspace whose code differs by OS (02-engine-core.md, Platform): each
//! entity here keeps one promise on every OS behind one interface, and `os`, one file for each OS
//! family, keeps it the way its OS can.

pub(crate) mod durable_name;
#[cfg(feature = "internals")]
pub(crate) mod file_link;
pub(crate) mod no_follow;
pub(crate) mod owner_only;
pub(crate) mod shared_port;
pub(crate) mod stop_signals;

// An OS of neither family names no file here, so the build fails where no promise is kept.
#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;
