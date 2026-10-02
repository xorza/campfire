//! What names package data from outside a package: paths inside a package, a package's
//! fingerprint, and the languages and message ids of its human text.

mod fingerprint;
mod language;
mod message_id;
mod package_path;

pub use fingerprint::Fingerprint;
pub use language::Language;
pub use message_id::MessageId;
pub use package_path::PackagePath;
