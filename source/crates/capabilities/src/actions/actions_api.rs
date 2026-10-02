use campfire_math::Num;
use campfire_script::rhai::Dynamic;
use campfire_sim::Capability;

use crate::actions::action_data_field::ActionDataField;
use crate::actions::actions_column::ActionsColumn;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_api::{ApiOwner, MemberSpec};
use crate::units::unit::Unit;

/// The script API of the action pipeline: what a unit's actions give it, and the fields of an
/// action's data that every kind reads.
#[derive(Debug)]
pub(crate) struct ActionsApi;

impl ActionsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| MemberSpec::field(ApiOwner::Unit, name, description);
        api.bind(
            field("target", "its attack's target, `()` with none"),
            |unit: &mut Unit| {
                let view = unit.view();
                ActionsColumn::target(view, unit.row_index())
                    .and_then(|target| view.unit(target))
                    .map_or(Dynamic::UNIT, Dynamic::from)
            },
        )
        .bind(
            field("attack_range", "its attack's range").capability(Capability::Combat),
            |unit: &mut Unit| -> Checked<Num> {
                ActionsColumn::attack_range(unit.view(), unit.row_index())
                    .ok_or_else(|| ApiError::NoAttack.fail().into())
            },
        )
        .action_fields(ActionDataField::of(None));
    }
}
