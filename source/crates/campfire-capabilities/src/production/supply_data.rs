use serde::Deserialize;

/// A unit type's `supply` section: what a unit of the type uses of its player's supply, and what
/// a complete one gives; each 0 when absent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyData {
    #[serde(default)]
    pub cost: u32,
    #[serde(default)]
    pub provides: u32,
}
