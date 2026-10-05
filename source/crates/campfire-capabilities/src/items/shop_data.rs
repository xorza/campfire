use serde::Deserialize;

use crate::values::declared_name::DeclaredName;
use crate::values::share::Share;

/// The mode's `[shop]`: the item types it sells, the player resource it takes, the marker tag
/// whose markers' regions of a carrier's team are its shops, and the share of an item's cost a
/// sale gives back, from 0 to 1.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShopData {
    pub items: Vec<DeclaredName>,
    pub resource: DeclaredName,
    pub at: DeclaredName,
    pub sell_share: Share,
}
