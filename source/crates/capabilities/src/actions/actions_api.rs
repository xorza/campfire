use crate::actions::action_data_field::ActionDataField;
use crate::scripts::api_builder::ApiBuilder;

/// The script API of the action pipeline: the fields of an action's data that every kind reads.
#[derive(Debug)]
pub(crate) struct ActionsApi;

impl ActionsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.action_fields(ActionDataField::of(None));
    }
}
