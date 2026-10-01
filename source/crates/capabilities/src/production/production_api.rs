use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::DataTable;

/// The data of `production` the release runs: a train's unit type, and a unit type's queue.
#[derive(Debug)]
pub(crate) struct ProductionApi;

impl ProductionApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.data(DataTable::Action, &["unit_type"], &[]).data(
            DataTable::Production,
            &["queue"],
            &[],
        );
    }
}
