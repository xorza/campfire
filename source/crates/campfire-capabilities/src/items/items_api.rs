use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::data_table::DataTable;

/// The data of `items`: the mode's item types and shop, and a unit type's inventory. Its MOBA cut
/// gives scripts no name: orders buy, sell and move items.
#[derive(Debug)]
pub(crate) struct ItemsApi;

impl ItemsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.data(DataTable::Mode, &["items", "shop"], &[])
            .data(
                DataTable::Item,
                &["cost", "components", "stack", "uses", "modifiers", "action"],
                &[],
            )
            .data(DataTable::Inventory, &["slots", "kind"], &[])
            .data(
                DataTable::Shop,
                &["items", "resource", "at", "sell_share"],
                &[],
            );
    }
}
