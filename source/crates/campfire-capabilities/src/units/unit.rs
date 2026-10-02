use campfire_script::rhai::{Dynamic, INT, ImmutableString, NativeCallContext};
use campfire_sim::StableId;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::state_value::StateValue;
use crate::units::script_view::View;
use crate::units::unit_row::UnitRow;
use crate::units::unit_state_call::UnitStateCall;
use crate::units::unit_state_column::UnitStateColumn;

/// A unit as a script holds it, `Unit` in scripts: its values as the view read them.
#[derive(Debug, Clone)]
pub(crate) struct Unit {
    pub(crate) id: StableId,
    /// Its place among the rows of its view, which does not change within a call.
    row: usize,
    view: View,
}

/// `unit.params`: the params of the unit's type, by name. `unit.params.aggro_range` reads
/// through the indexer, as Rhai tries one for a property with no getter.
#[derive(Debug, Clone)]
pub(crate) struct UnitParams(Unit);

/// `unit.state`: the unit's script state fields, by name, to read and write.
#[derive(Debug, Clone)]
pub(crate) struct UnitStateAccess(Unit);

impl Unit {
    /// Only the view makes a handle, of a unit it read: `View::unit` for any other code.
    pub(super) const fn new(id: StableId, row: usize, view: View) -> Unit {
        Unit { id, row, view }
    }

    pub(crate) const fn view(&self) -> &View {
        &self.view
    }

    /// The unit's place among its view's rows.
    pub(crate) const fn row_index(&self) -> usize {
        self.row
    }

    /// The unit's row. A handle comes only from its view's rows, and lives only within a call.
    pub(crate) fn row(&self) -> UnitRow {
        self.view.row_at(self.row)
    }

    /// The `Unit` handle's fields and methods.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        Unit::register_fields(api);
        Unit::register_methods(api);
    }

    fn register_fields(api: &mut ApiBuilder<'_>) {
        let field = |name, description| MemberSpec::field(ApiOwner::Unit, name, description);
        api.ty::<Unit>("Unit")
            .bind(field("pos", "where it stands"), |unit: &mut Unit| {
                unit.row().pos
            })
            .bind(
                field("radius", "its body's radius, 0 with no body"),
                |unit: &mut Unit| unit.row().radius,
            )
            .bind(field("alive", "whether it lives"), |unit: &mut Unit| {
                unit.row().alive
            })
            .bind(
                field("is_avatar", "whether it is an avatar"),
                |unit: &mut Unit| unit.row().is_avatar(),
            )
            .bind(
                field("params", "its unit type's params, unresolved"),
                |unit: &mut Unit| UnitParams(unit.clone()),
            )
            .bind(
                field(
                    "state",
                    "its script state, by name, which a call may write and read back",
                ),
                |unit: &mut Unit| UnitStateAccess(unit.clone()),
            )
            .bind(field("team", "its team's name"), |unit: &mut Unit| {
                unit.view.team_name(unit.row().team)
            })
            .bind(
                field("unit_type", "its unit type's name"),
                |unit: &mut Unit| unit.view.unit_type_name(&unit.row()),
            )
            .bind(
                field("owner", "its player's slot, `()` with none"),
                |unit: &mut Unit| {
                    unit.row().owner.map_or(Dynamic::UNIT, |slot| {
                        Dynamic::from_int(INT::from(slot.get()))
                    })
                },
            )
            .bind(
                field(
                    "spawn_pos",
                    "where it spawned, where it respawns; `()` with none",
                ),
                |unit: &mut Unit| unit.row().spawn.map_or(Dynamic::UNIT, Dynamic::from),
            );
        api.ty::<UnitParams>("UnitParams")
            .index(|params: &mut UnitParams, name: ImmutableString| params.get(&name));
        api.ty::<UnitStateAccess>("UnitState")
            .index(
                |call: NativeCallContext<'_>,
                 state: &mut UnitStateAccess,
                 name: ImmutableString| { state.get(&Ctx::of_call(&call), &name) },
            )
            .index_set(
                |call: NativeCallContext<'_>,
                 state: &mut UnitStateAccess,
                 name: ImmutableString,
                 value: Dynamic| { state.set(&Ctx::of_call(&call), &name, &value) },
            );
    }

    fn register_methods(api: &mut ApiBuilder<'_>) {
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::Unit, name, signature, description)
        };
        api.bind(
            method(
                "has_tag",
                "(tag)",
                "whether it has the tag, of its type or a modifier",
            )
            .name(0, NameKind::Tag),
            |unit: &mut Unit, name: &str| -> Checked<bool> {
                let tag = unit.view.tag_named(name).map_err(ApiError::fail)?;
                Ok(unit.row().tags.tags.contains(tag))
            },
        )
        .bind(
            method(
                "is_enemy_of",
                "(unit)",
                "whether its team may attack the other's, hostile or neutral",
            ),
            |unit: &mut Unit, other: Unit| {
                let attitude = unit.view.attitude(unit.row().team, other.row().team);
                attitude.may_attack()
            },
        )
        .bind(
            MemberSpec::operator(ApiOwner::Unit, "==", "whether the two are one unit"),
            |a: Unit, b: Unit| a.id == b.id,
        )
        .bind(
            MemberSpec::operator(ApiOwner::Unit, "!=", "whether the two are two units"),
            |a: Unit, b: Unit| a.id != b.id,
        );
    }
}

impl UnitParams {
    /// The param `name`; one the unit's type does not declare fails the call.
    fn get(&self, name: &str) -> Checked<Dynamic> {
        let unit = &self.0;
        unit.view
            .param_named(&unit.row(), name)
            .ok_or_else(|| ApiError::UnknownParam.fail().into())
    }
}

impl UnitStateAccess {
    /// The field `name`, as the call last wrote it or else as the view holds it; one the unit's
    /// type does not declare fails the call.
    fn get(&self, ctx: &Ctx, name: &str) -> Checked<Dynamic> {
        let unit = &self.0;
        let field = UnitStateColumn::field(&unit.view, unit.row().unit_type, name)?;
        let frame = ctx.frame();
        Ok(match UnitStateCall::of(&frame).written(unit.id, field.at) {
            Some(value) => value.to_dynamic(&unit.view),
            None => UnitStateColumn::read(&unit.view, unit.row, field.at),
        })
    }

    /// Writes `value` to the field `name`, for the call to read back and to apply when it ends;
    /// a field the unit's type does not declare, a value of another type, and a pure hook's call
    /// fail the call.
    fn set(&self, ctx: &Ctx, name: &str, value: &Dynamic) -> Checked<()> {
        let unit = &self.0;
        let field = UnitStateColumn::field(&unit.view, unit.row().unit_type, name)?;
        let value = StateValue::from_dynamic(field.kind, value)
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        let mut frame = ctx.write()?;
        UnitStateCall::of_mut(&mut frame).write(unit.id, field.at, value);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_script::rhai::Dynamic;
    use campfire_sim::StableId;

    use crate::units::unit::Unit;

    impl Unit {
        /// The stable ids of `value`, a unit, a list of units, or `()` for none.
        pub(crate) fn ids(value: Dynamic) -> Vec<StableId> {
            let units = match value.clone().try_cast::<Vec<Dynamic>>() {
                Some(units) => units,
                None if value.is_unit() => Vec::new(),
                None => vec![value],
            };
            units
                .into_iter()
                .map(|unit| unit.try_cast::<Unit>().unwrap().id)
                .collect()
        }
    }
}
