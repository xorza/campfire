use campfire_script::{ApiBuilder, MemberDecl};

use crate::inventory_data::InventoryData;
use crate::item_data::ItemData;
use crate::shop_data::ShopData;

/// The data of `items`: the mode's item types, and a unit type's inventory and shop. Its MOBA cut
/// gives scripts no name it runs: orders buy, sell and move items.
#[derive(Debug)]
pub struct ItemsApi;

impl ItemsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.plan(MemberDecl::call(
            "give_item",
            &[&["unit", "id"]],
            "gives `unit` the item `id`, in its first slot that takes it",
        ))
        .plan(MemberDecl::call(
            "drop",
            &[&["unit", "slot"]],
            "drops the item in `unit`'s inventory slot `slot` at its feet",
        ))
        .data::<ItemData>(&[])
        .data::<InventoryData>(&[])
        .data::<ShopData>(&[]);
    }
}
