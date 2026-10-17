use campfire_math::Num;
use campfire_script::{
    ApiBuilder, ApiError, ApiOwner, Checked, DataTable, MemberSpec, ScriptValue, Unit,
};
use campfire_sim::Capability;

use crate::action_data_field::ActionDataField;
use crate::actions_column::ActionsColumn;
use crate::listed_effect::effect_data::EffectData;
use crate::planned_effect::PlannedEffect;

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
                    .map_or(ScriptValue::UNIT, ScriptValue::of)
            },
        )
        .bind(
            field("attack_range", "its attack's range").capability(Capability::Combat),
            |unit: &mut Unit| -> Checked<Num> {
                ActionsColumn::attack_range(unit.view(), unit.row_index())
                    .ok_or_else(|| ApiError::NoAttack.fail().into())
            },
        )
        .action_fields(ActionDataField::of(None))
        .shared_data(
            DataTable::Effect,
            &[EffectData::TO],
            &PlannedEffect::ALL.map(PlannedEffect::name),
        );
    }
}
