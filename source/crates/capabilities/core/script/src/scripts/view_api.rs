use campfire_geometry::Shape;
use campfire_values::{Named, Param, Tag, Team, UnitType};

use crate::scripts::api_holder::ApiHolder;
use crate::scripts::api_holder::ScriptHandle;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::id_name::IdName;
use crate::scripts::member_decl::MemberDecl;
use crate::scripts::player_handle::PlayerHandle;
use crate::scripts::script_api::api_builder::ApiBuilder;
use crate::scripts::script_position::ScriptPosition;
use crate::scripts::script_radius::ScriptRadius;
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
        Self::unit_fields(api);
        Self::unit_methods(api);
    }

    fn unit_fields(api: &mut ApiBuilder<'_>) {
        let field =
            |name, description| MemberDecl::field(ApiHolder::of::<Unit>(), name, description);
        api.ty::<Unit>()
            .bind(field("position", "where it stands"), |unit: &mut Unit| {
                unit.read(|row| row.position)
            })
            .bind(
                field(
                    "radius",
                    "its body's radius, 0 with no body, `()` for a box",
                ),
                |unit: &mut Unit| -> Checked<ScriptValue> {
                    Ok(match unit.read(|row| row.shape)? {
                        Shape::Circle(radius) => ScriptValue::of(radius.get()),
                        Shape::Box(_) => ScriptValue::UNIT,
                    })
                },
            )
            .bind(field("is_alive", "whether it lives"), |unit: &mut Unit| {
                unit.read(|row| row.alive)
            })
            .bind(
                field("params", "its unit type's params, unresolved"),
                |unit: &mut Unit| UnitParams(unit.clone()),
            )
            .bind(
                field("team", "its team's name"),
                |unit: &mut Unit| -> Checked<IdName<Team>> {
                    Ok(unit.view().names().name(unit.read(|row| row.team)?))
                },
            )
            .bind(
                field("unit_type", "its unit type's name"),
                |unit: &mut Unit| -> Checked<IdName<UnitType>> {
                    Ok(unit.view().names().name(unit.read(|row| row.unit_type)?))
                },
            )
            .bind(
                field("owner", "its player, `()` with none"),
                |unit: &mut Unit| -> Checked<ScriptValue> {
                    let owner = unit.read(|row| row.owner)?;
                    Ok(owner.map_or(ScriptValue::UNIT, |slot| {
                        ScriptValue::of(PlayerHandle::new(slot))
                    }))
                },
            )
            .bind(
                field(
                    "spawn_pos",
                    "where it spawned, where it respawns; `()` with none",
                ),
                |unit: &mut Unit| -> Checked<ScriptValue> {
                    let spawn = unit.read(|row| row.spawn)?;
                    Ok(spawn.map_or(ScriptValue::UNIT, ScriptValue::of))
                },
            );
        api.ty::<UnitParams>()
            .index(|_, params: &mut UnitParams, name: &str| params.get(name));
    }

    fn unit_methods(api: &mut ApiBuilder<'_>) {
        let method = |name, signature, description| {
            MemberDecl::method(ApiHolder::of::<Unit>(), name, signature, description)
        };
        api.method(
            method(
                "has_tag",
                &[&["tag"]],
                "whether it has the tag, of its type or a modifier",
            ),
            |unit: Unit, tag: Tag| unit.read(|row| row.tags.tags.contains(tag)),
        )
        .method(
            method(
                "is_enemy_of",
                &[&["unit"]],
                "whether its team may attack the other's, hostile or neutral",
            ),
            |unit: Unit, other: Unit| {
                let (team, other) = (unit.read(|row| row.team)?, other.read(|row| row.team)?);
                let relation = unit.view().relation(team, other);
                Ok(relation.may_attack())
            },
        )
        .method(
            MemberDecl::operator(
                ApiHolder::of::<Unit>(),
                "==",
                "whether the two are one unit",
            ),
            |a: Unit, b: Unit| Ok(a.id == b.id),
        )
        .method(
            MemberDecl::operator(
                ApiHolder::of::<Unit>(),
                "!=",
                "whether the two are two units",
            ),
            |a: Unit, b: Unit| Ok(a.id != b.id),
        );
    }

    /// `ctx.find`.
    pub fn register_queries(api: &mut ApiBuilder<'_>) {
        let find = MemberDecl::call(
            "find",
            &[&["of", "position", "radius", "filter"]],
            "the living targets whose bodies come within `radius` of `position`, as an area's, that `filter` selects for `of`, seen or not, by stable id",
        );
        api.call(
            find,
            |ctx: &mut Ctx,
             of: Unit,
             position: ScriptPosition,
             radius: ScriptRadius,
             filter: Filter| {
                ctx.view()
                    .find(&of, position.get(), radius.get(), &filter, |_, _| true)
            },
        );
    }
}

impl UnitParams {
    /// The param `name`; one the unit's type does not declare fails the call.
    fn get(&self, name: &str) -> Checked<ScriptValue> {
        let unit = &self.0;
        let unit_type = unit.read(|row| row.unit_type)?;
        let param = unit.view().names().param(unit_type, name);
        let param = param.ok_or_else(|| ApiError::Unknown(Param::NAMES).fail())?;
        Ok(ScriptValue::from(param))
    }
}

impl ScriptHandle for UnitParams {
    const NAME: &'static str = "UnitParams";
    const TITLE: &'static str = "A unit's params";
}
