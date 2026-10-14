use campfire_script::{ApiBuilder, ApiError, Checked, Ctx, ScriptValue, StateValue};
use campfire_sim::StableId;
use campfire_values::UnitType;

use crate::units::new_unit::NewUnit;
use crate::units::units_call::UnitsCall;
use crate::units::units_column::UnitsColumn;

/// `unit.state`, `UnitState` in scripts: the script state fields of a unit a script holds, by
/// name, to read and write; of a unit the view read, or of one the call creates, which has no
/// row until it spawns.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitStateAccess {
    id: StableId,
    unit_type: UnitType,
    /// Its row in the view; none for a unit the call creates.
    row: Option<usize>,
}

impl UnitStateAccess {
    /// The state of the unit `id` of `unit_type` in row `row` of the view.
    pub(crate) const fn of_row(id: StableId, unit_type: UnitType, row: usize) -> UnitStateAccess {
        UnitStateAccess {
            id,
            unit_type,
            row: Some(row),
        }
    }

    /// The state of `unit`, which the call creates.
    pub(crate) const fn of_new(unit: NewUnit) -> UnitStateAccess {
        UnitStateAccess {
            id: unit.id,
            unit_type: unit.unit_type,
            row: None,
        }
    }

    /// Indexes `UnitState` by field name, to read and to write.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.ty::<UnitStateAccess>("UnitState")
            .index(|args, state: &mut UnitStateAccess, name| state.get(args.get(), name))
            .index_set(|args, state: &mut UnitStateAccess, name, value| {
                state.set(args.get(), name, &value)
            });
    }

    /// The field `name`, as the call last wrote it, or else as the view holds it, or at its
    /// type's default for a unit the call creates; one the unit's type does not declare fails
    /// the call.
    fn get(&self, ctx: &Ctx, name: &str) -> Checked<ScriptValue> {
        let view = ctx.view();
        let field = UnitsColumn::field(view, self.unit_type, name)?;
        if let Some(value) = UnitsCall::of(&ctx.frame()).written(self.id, field.at) {
            return Ok(value.to_value(view));
        }
        if let Some(row) = self.row {
            return Ok(UnitsColumn::read(view, row, field.at));
        }
        Ok(UnitsColumn::initial(view, self.unit_type, field.at))
    }

    /// Writes `value` to the field `name`, for the call to read back and to apply when it ends,
    /// or as the unit spawns; a field the unit's type does not declare, a value of another
    /// type, and a pure hook's call fail the call.
    fn set(&self, ctx: &Ctx, name: &str, value: &ScriptValue) -> Checked<()> {
        let field = UnitsColumn::field(ctx.view(), self.unit_type, name)?;
        let value = StateValue::from_value(field.kind, value)
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        let mut frame = ctx.write()?;
        UnitsCall::of_mut(&mut frame).write(self.id, field, value);
        Ok(())
    }
}
