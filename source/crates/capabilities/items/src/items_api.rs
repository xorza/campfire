use campfire_script::{ApiBuilder, DataTable, MemberSpec};

use crate::inventory_data::InventoryData;
use crate::item_data::ItemData;
use crate::shop_data::ShopData;

/// The data of `items`: the mode's item types, and a unit type's inventory and shop. Its MOBA cut
/// gives scripts no name it runs: orders buy, sell and move items.
#[derive(Debug)]
pub struct ItemsApi;

impl ItemsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.plan(MemberSpec::call(
            "give_item",
            &[&["unit", "id"]],
            "gives `unit` the item `id`, in its first slot that takes it",
        ))
        .plan(MemberSpec::call(
            "drop",
            &[&["unit", "slot"]],
            "drops the item in `unit`'s inventory slot `slot` at its feet",
        ))
        .shared_data(DataTable::Mode, &["items"], &[])
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
