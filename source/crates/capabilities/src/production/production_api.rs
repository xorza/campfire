use campfire_sim::Capability;

use crate::actions::action_data_field::ActionDataField;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::data_table::DataTable;

/// The data of `production` the release runs: a train's unit type, and a unit type's queue.
#[derive(Debug)]
pub(crate) struct ProductionApi;

impl ProductionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.action_fields(ActionDataField::of(Some(Capability::Production)))
            .data(DataTable::Production, &["queue"], &[]);
    }
}
