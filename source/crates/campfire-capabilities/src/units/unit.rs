use campfire_script::rhai::{Dynamic, INT};
use campfire_sim::StableId;

use crate::geometry::shape::Shape;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::units::script_view::View;
use crate::units::unit_row::UnitRow;
use crate::units::unit_state_access::UnitStateAccess;

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

    /// What `read` reads of the unit's row, in place. A handle comes only from its view's rows,
    /// and lives only within a call.
    pub(crate) fn read<R>(&self, read: impl FnOnce(&UnitRow) -> R) -> R {
        self.view.read_row(self.row, read)
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
                unit.read(|row| row.pos)
            })
            .bind(
                field(
                    "radius",
                    "its body's radius, 0 with no body, `()` for a box",
                ),
                |unit: &mut Unit| match unit.read(|row| row.shape) {
                    Shape::Circle(radius) => Dynamic::from(radius),
                    Shape::Box(_) => Dynamic::UNIT,
                },
            )
            .bind(field("alive", "whether it lives"), |unit: &mut Unit| {
                unit.read(|row| row.alive)
            })
            .bind(
                field("is_avatar", "whether it is an avatar"),
                |unit: &mut Unit| unit.read(UnitRow::is_avatar),
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
                |unit: &mut Unit| {
                    UnitStateAccess::of_row(unit.id, unit.read(|row| row.unit_type), unit.row)
                },
            )
            .bind(field("team", "its team's name"), |unit: &mut Unit| {
                unit.view.team_name(unit.read(|row| row.team))
            })
            .bind(
                field("unit_type", "its unit type's name"),
                |unit: &mut Unit| unit.view.unit_type_name(unit.read(|row| row.unit_type)),
            )
            .bind(
                field("owner", "its player's slot, `()` with none"),
                |unit: &mut Unit| {
                    unit.read(|row| row.owner).map_or(Dynamic::UNIT, |slot| {
                        Dynamic::from_int(INT::from(slot.get()))
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
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            );
        api.ty::<UnitParams>("UnitParams")
            .index(|params: &mut UnitParams, name: &str| params.get(name));
        UnitStateAccess::register(api);
    }

    fn register_methods(api: &mut ApiBuilder<'_>) {
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::Unit, name, signature, description)
        };
        api.bind(
            method(
                "has_tag",
                &[&["tag"]],
                "whether it has the tag, of its type or a modifier",
            )
            .name(0, NameKind::Tag),
            |unit: Unit, name: &str| -> Checked<bool> {
                let tag = unit.view.tag_named(name).map_err(ApiError::fail)?;
                Ok(unit.read(|row| row.tags.tags.contains(tag)))
            },
        )
        .bind(
            method(
                "is_enemy_of",
                &[&["unit"]],
                "whether its team may attack the other's, hostile or neutral",
            ),
            |unit: Unit, other: Unit| {
                let teams = [&unit, &other].map(|unit| unit.read(|row| row.team));
                let attitude = unit.view.attitude(teams[0], teams[1]);
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
        unit.read(|row| row.unit_type)
            .and_then(|unit_type| unit.view.param_named(unit_type, name))
            .ok_or_else(|| ApiError::UnknownParam.fail().into())
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
