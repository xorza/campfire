use serde::Deserialize;

/// Where a dependency is: in the workspace, a path relative to the mode's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub path: String,
}
