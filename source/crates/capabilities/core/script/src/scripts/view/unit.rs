use campfire_geometry::Shape;
use campfire_sim::StableId;
use campfire_values::Tag;

use crate::plain_script_arg;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::view::View;
use crate::scripts::view::unit_row::UnitRow;

/// A unit as a script holds it, `Unit` in scripts: its values as the view read them.
#[derive(Debug, Clone)]
pub struct Unit {
    pub id: StableId,
    /// Its place among the rows of its view, which does not change within a call.
    row: usize,
    view: View,
}

/// `unit.params`: the params of the unit's type, by name. `unit.params.aggro_range` reads
/// through the indexer, as Rhai tries one for a property with no getter.
#[derive(Debug, Clone)]
pub(crate) struct UnitParams(Unit);

impl Unit {
    /// Only the view makes a handle, of a unit it read: `View::unit` for any other code.
    pub const fn new(id: StableId, row: usize, view: View) -> Unit {
        Unit { id, row, view }
    }

    pub const fn view(&self) -> &View {
        &self.view
    }

    /// The unit's place among its view's rows.
    pub const fn row_index(&self) -> usize {
        self.row
    }

    /// What `read` reads of the unit's row, in place. A handle comes only from its view's rows,
    /// and lives only within a call.
    pub fn read<R>(&self, read: impl FnOnce(&UnitRow) -> R) -> R {
        self.view.read_row(self.row, read)
    }

    /// The `Unit` handle's fields and methods.
    pub fn register(api: &mut ApiBuilder<'_>) {
        Unit::register_fields(api);
        Unit::register_methods(api);
    }

    fn register_fields(api: &mut ApiBuilder<'_>) {
        let field = |name, description| MemberSpec::field(ApiOwner::Unit, name, description);
        api.ty::<Unit>("Unit")
            .bind(field("pos", "where it stands"), |unit: &mut Unit| {
                unit.read(|row| row.pos)
            })
            .bind(
                field(
                    "radius",
                    "its body's radius, 0 with no body, `()` for a box",
                ),
                |unit: &mut Unit| match unit.read(|row| row.shape) {
                    Shape::Circle(radius) => ScriptValue::of(radius),
                    Shape::Box(_) => ScriptValue::UNIT,
                },
            )
            .bind(field("alive", "whether it lives"), |unit: &mut Unit| {
                unit.read(|row| row.alive)
            })
            .bind(
                field("params", "its unit type's params, unresolved"),
                |unit: &mut Unit| UnitParams(unit.clone()),
            )
            .bind(
                field("team", "its team's name"),
                |unit: &mut Unit| -> Checked<ScriptValue> {
                    let name = unit.view.names().name(unit.read(|row| row.team));
                    name.map(ScriptValue::of)
                        .ok_or_else(|| ApiError::Unknown(NameKind::Team).fail().into())
                },
            )
            .bind(
                field("unit_type", "its unit type's name"),
                |unit: &mut Unit| {
                    let unit_type = unit.read(|row| row.unit_type);
                    let name = unit.view.names().name(unit_type);
                    name.map_or(ScriptValue::UNIT, ScriptValue::of)
                },
            )
            .bind(
                field("owner", "its player's slot, `()` with none"),
                |unit: &mut Unit| {
                    unit.read(|row| row.owner)
                        .map_or(ScriptValue::UNIT, |slot| {
                            ScriptValue::of(i64::from(slot.get()))
                        })
                },
            )
            .bind(
                field(
                    "spawn_pos",
                    "where it spawned, where it respawns; `()` with none",
                ),
                |unit: &mut Unit| {
                    unit.read(|row| row.spawn)
                        .map_or(ScriptValue::UNIT, ScriptValue::of)
                },
            );
        api.ty::<UnitParams>("UnitParams")
            .index(|_, params: &mut UnitParams, name: &str| params.get(name));
    }

    fn register_methods(api: &mut ApiBuilder<'_>) {
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::Unit, name, signature, description)
        };
        api.method(
            method(
                "has_tag",
                &[&["tag"]],
                "whether it has the tag, of its type or a modifier",
            ),
            |unit: Unit, tag: Tag| Ok(unit.read(|row| row.tags.tags.contains(tag))),
        )
        .method(
            method(
                "is_enemy_of",
                &[&["unit"]],
                "whether its team may attack the other's, hostile or neutral",
            ),
            |unit: Unit, other: Unit| {
                let teams = [&unit, &other].map(|unit| unit.read(|row| row.team));
                let relation = unit.view.relation(teams[0], teams[1]);
                Ok(relation.may_attack())
            },
        )
        .method(
            MemberSpec::operator(ApiOwner::Unit, "==", "whether the two are one unit"),
            |a: Unit, b: Unit| Ok(a.id == b.id),
        )
        .method(
            MemberSpec::operator(ApiOwner::Unit, "!=", "whether the two are two units"),
            |a: Unit, b: Unit| Ok(a.id != b.id),
        );
    }
}

impl UnitParams {
    /// The param `name`; one the unit's type does not declare fails the call.
    fn get(&self, name: &str) -> Checked<ScriptValue> {
        let unit = &self.0;
        let unit_type = unit.read(|row| row.unit_type);
        let param = unit.view.names().param(unit_type, name);
        let param = param.ok_or_else(|| ApiError::Unknown(NameKind::Param).fail())?;
        Ok(ScriptValue::from(param))
    }
}

plain_script_arg!(Unit);

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_sim::StableId;

    use crate::scripts::script_list::ScriptList;
    use crate::scripts::script_value::ScriptValue;
    use crate::scripts::view::unit::Unit;

    impl Unit {
        /// The stable ids of `value`, a unit, a list of units, or `()` for none.
        pub fn ids(value: ScriptValue) -> Vec<StableId> {
            let units = match value.clone().try_cast::<ScriptList>() {
                Some(units) => units,
                None if value.is_unit() => ScriptList::default(),
                None => ScriptList::from([value]),
            };
            units
                .into_iter()
                .map(|unit| unit.try_cast::<Unit>().unwrap().id)
                .collect()
        }
    }
}
