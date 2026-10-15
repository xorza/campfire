use campfire_geometry::Shape;
use campfire_math::Num;
use campfire_sim::Position;
use campfire_values::Tag;

use crate::plain_script_arg;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::view::filter::Filter;
use crate::scripts::view::unit::Unit;

/// The script API of the view: the `Unit` handle's fields and methods, and `ctx.find`.
#[derive(Debug)]
pub struct ViewApi;

/// `unit.params`: the params of the unit's type, by name. `unit.params.aggro_range` reads
/// through the indexer, as Rhai tries one for a property with no getter.
#[derive(Debug, Clone)]
pub(crate) struct UnitParams(Unit);

impl ViewApi {
    /// The `Unit` handle's fields and methods.
    pub fn register_unit(api: &mut ApiBuilder<'_>) {
        ViewApi::unit_fields(api);
        ViewApi::unit_methods(api);
    }

    fn unit_fields(api: &mut ApiBuilder<'_>) {
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
                    let name = unit.view().names().name(unit.read(|row| row.team));
                    name.map(ScriptValue::of)
                        .ok_or_else(|| ApiError::Unknown(NameKind::Team).fail().into())
                },
            )
            .bind(
                field("unit_type", "its unit type's name"),
                |unit: &mut Unit| {
                    let unit_type = unit.read(|row| row.unit_type);
                    let name = unit.view().names().name(unit_type);
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

    fn unit_methods(api: &mut ApiBuilder<'_>) {
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
                let relation = unit.view().relation(teams[0], teams[1]);
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

    /// `ctx.find`.
    pub fn register_queries(api: &mut ApiBuilder<'_>) {
        let find = MemberSpec::call(
            "find",
            &[&["of", "pos", "radius", "filter"]],
            "the living targets whose bodies come within `radius` of `pos`, as an area's, that `filter` selects for `of`, seen or not, by stable id",
        );
        api.call(
            find,
            |ctx: &mut Ctx, of: Unit, pos: Position, radius: Num, filter: Filter| {
                ctx.view().find(&of, pos, radius, &filter, |_, _| true)
            },
        );
    }
}

impl UnitParams {
    /// The param `name`; one the unit's type does not declare fails the call.
    fn get(&self, name: &str) -> Checked<ScriptValue> {
        let unit = &self.0;
        let unit_type = unit.read(|row| row.unit_type);
        let param = unit.view().names().param(unit_type, name);
        let param = param.ok_or_else(|| ApiError::Unknown(NameKind::Param).fail())?;
        Ok(ScriptValue::from(param))
    }
}

plain_script_arg!(Unit);
