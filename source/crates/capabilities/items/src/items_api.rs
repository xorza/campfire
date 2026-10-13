use campfire_core::{ApiBuilder, DataTable};

use crate::inventory_data::InventoryData;
use crate::item_data::ItemData;
use crate::shop_data::ShopData;

/// The data of `items`: the mode's item types and shop, and a unit type's inventory. Its MOBA cut
/// gives scripts no name: orders buy, sell and move items.
#[derive(Debug)]
pub struct ItemsApi;

impl ItemsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.shared_data(DataTable::Mode, &["items", "shop"], &[])
            .data::<ItemData>(
                &["cost", "components", "stack", "uses", "modifiers", "action"],
                &[],
            )
            .data::<InventoryData>(&["slots", "kind"], &[])
            .shared_data(DataTable::UnitType, &["inventory"], &[])
            .data::<ShopData>(&["items", "resource", "at", "sell_share"], &[]);
    }
}
