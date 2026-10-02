use serde::Deserialize;

use crate::values::package_path::PackagePath;

/// A unit type's AI as its `orders` section declares it: the script whose `on_think` runs, and how
/// often, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiData {
    pub ai: PackagePath,
    pub think_ms: u64,
}
