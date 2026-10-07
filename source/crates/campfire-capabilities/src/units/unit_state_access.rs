use campfire_script::rhai::Dynamic;
use campfire_sim::StableId;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::state_value::StateValue;
use crate::units::new_unit::NewUnit;
use crate::units::unit_type::UnitType;
use crate::units::units_call::UnitsCall;
use crate::units::units_column::UnitsColumn;

/// `unit.state`, `UnitState` in scripts: the script state fields of a unit a script holds, by
/// name, to read and write; of a unit the view read, or of one the call creates, which has no
/// row until it spawns.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitStateAccess {
    unit: StableId,
    unit_type: Option<UnitType>,
    /// Its row in the view; none for a unit the call creates.
    row: Option<usize>,
}

impl UnitStateAccess {
    /// The state of the unit `unit` of `unit_type` in row `row` of the view.
    pub(crate) const fn of_row(
        unit: StableId,
        unit_type: Option<UnitType>,
        row: usize,
    ) -> UnitStateAccess {
        UnitStateAccess {
            unit,
            unit_type,
            row: Some(row),
        }
    }

    /// The state of `unit`, which the call creates.
    pub(crate) const fn of_new(unit: NewUnit) -> UnitStateAccess {
        UnitStateAccess {
            unit: unit.id,
            unit_type: Some(unit.unit_type),
            row: None,
        }
    }

    /// Indexes `UnitState` by field name, to read and to write.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.ty::<UnitStateAccess>("UnitState")
            .index_in_call(|call, state: &mut UnitStateAccess, name| {
                state.get(&Ctx::of_call(&call), name)
            })
            .index_set_in_call(|call, state: &mut UnitStateAccess, name, value| {
                state.set(&Ctx::of_call(&call), name, &value)
            });
    }

    /// The field `name`, as the call last wrote it, or else as the view holds it, or at its
    /// type's default for a unit the call creates; one the unit's type does not declare fails
    /// the call.
    fn get(&self, ctx: &Ctx, name: &str) -> Checked<Dynamic> {
        let view = ctx.view();
        let field = UnitsColumn::field(view, self.unit_type, name)?;
        if let Some(value) = UnitsCall::of(&ctx.frame()).written(self.unit, field.at) {
            return Ok(value.to_dynamic(view));
        }
        if let Some(row) = self.row {
            return Ok(UnitsColumn::read(view, row, field.at));
        }
        let unit_type = self
            .unit_type
            .expect("a unit the call creates has its type");
        Ok(UnitsColumn::initial(view, unit_type, field.at))
    }

    /// Writes `value` to the field `name`, for the call to read back and to apply when it ends,
    /// or as the unit spawns; a field the unit's type does not declare, a value of another
    /// type, and a pure hook's call fail the call.
    fn set(&self, ctx: &Ctx, name: &str, value: &Dynamic) -> Checked<()> {
        let field = UnitsColumn::field(ctx.view(), self.unit_type, name)?;
        let value = StateValue::from_dynamic(field.kind, value)
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        let mut frame = ctx.write()?;
        UnitsCall::of_mut(&mut frame).write(self.unit, field.at, value);
        Ok(())
    }
}
