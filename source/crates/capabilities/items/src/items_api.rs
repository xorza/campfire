use campfire_script::{ApiBuilder, DataTable};

use crate::inventory_data::InventoryData;
use crate::item_data::ItemData;
use crate::shop_data::ShopData;

/// The data of `items`: the mode's item types, and a unit type's inventory and shop. Its MOBA cut
/// gives scripts no name: orders buy, sell and move items.
#[derive(Debug)]
pub struct ItemsApi;

impl ItemsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.shared_data(DataTable::Mode, &["items"], &[])
            .data::<ItemData>(
                &[
                    "name",
                    "cost",
                    "components",
                    "stack",
                    "uses",
                    "modifiers",
                    "action",
                ],
                &[],
            )
            .data::<InventoryData>(&["slots", "kind"], &[])
            .shared_data(DataTable::UnitType, &["inventory", "shop"], &[])
            .data::<ShopData>(&["items", "resource", "reach", "buyers", "sell_share"], &[]);
    }
}
