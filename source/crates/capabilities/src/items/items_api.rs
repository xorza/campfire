use crate::items::inventory_data::InventoryData;
use crate::items::item_data::ItemData;
use crate::items::shop_data::ShopData;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::data_table::DataTable;

/// The data of `items`: the mode's item types and shop, and a unit type's inventory. Its MOBA cut
/// gives scripts no name: orders buy, sell and move items.
#[derive(Debug)]
pub(crate) struct ItemsApi;

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
