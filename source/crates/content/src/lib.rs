//! What names package data from outside a package: paths inside a package, and a package's
//! fingerprint.

mod fingerprint;
mod package_path;

pub use fingerprint::Fingerprint;
pub use package_path::PackagePath;
