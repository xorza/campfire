use campfire_script::{ApiBuilder, DataTable};

use crate::use_data::UseData;
use crate::uses_data::UsesData;

/// The data of `interaction`: a unit type's `use` and `uses`. Its first cut gives scripts no
/// name: the `use` order starts a use, and the use's action runs its hooks.
#[derive(Debug)]
pub struct InteractionApi;

impl InteractionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.shared_data(DataTable::UnitType, &["use", "uses"], &[])
            .data::<UseData>(&["action", "users"], &[])
            .data::<UsesData>(&["kind"], &[]);
    }
}
