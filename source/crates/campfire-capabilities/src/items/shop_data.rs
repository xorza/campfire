use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// The mode's `[shop]`: the item types it sells, the player resource it takes, the marker tag
/// whose markers' regions of a carrier's team are its shops, and the share of an item's cost a
/// sale gives back, from 0 to 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopData {
    pub items: Vec<DeclaredName>,
    pub resource: DeclaredName,
    pub at: DeclaredName,
    pub sell_share: Num,
}

/// A sell share from 0 to 1, or the section fails to read.
impl<'de> Deserialize<'de> for ShopData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ShopData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            items: Vec<DeclaredName>,
            resource: DeclaredName,
            at: DeclaredName,
            sell_share: Scalar,
        }
        let fields = Fields::deserialize(deserializer)?;
        let sell_share = fields
            .sell_share
            .to_num()
            .filter(|share| (Num::ZERO..=Num::ONE).contains(share))
            .ok_or_else(|| D::Error::custom("a sell share is a number from 0 to 1"))?;
        Ok(ShopData {
            items: fields.items,
            resource: fields.resource,
            at: fields.at,
            sell_share,
        })
    }
}
