use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_values::{DeclaredName, FilterData, Scalar, Share};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A unit type's `shop` section, which makes its units shops: the item types it sells, the player
/// resource it takes, how far from its body's edge it serves, the units it serves, and the share
/// of an item's cost a sale gives back, from 0 to 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopData {
    pub items: Vec<DeclaredName>,
    pub resource: DeclaredName,
    pub reach: Num,
    pub buyers: FilterData,
    pub sell_share: Share,
}

/// Data is untrusted, so a negative reach fails to read.
impl<'de> Deserialize<'de> for ShopData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ShopData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            items: Vec<DeclaredName>,
            resource: DeclaredName,
            reach: Scalar,
            buyers: FilterData,
            sell_share: Share,
        }
        let fields = Fields::deserialize(deserializer)?;
        let reach = fields
            .reach
            .to_num()
            .filter(|reach| *reach >= Num::ZERO)
            .ok_or_else(|| D::Error::custom("a shop's reach is a number that is not negative"))?;
        Ok(ShopData {
            items: fields.items,
            resource: fields.resource,
            reach,
            buyers: fields.buyers,
            sell_share: fields.sell_share,
        })
    }
}

impl DataSection for ShopData {
    const TABLE: DataTable = DataTable::Shop;
}
