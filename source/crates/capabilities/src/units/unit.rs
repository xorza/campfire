use campfire_math::{Num, Vec3};
use campfire_script::NumError;
use campfire_script::Raised;
use campfire_script::rhai::{Dynamic, INT, ImmutableString, NativeCallContext};
use campfire_sim::{Capability, Position, StableId};

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::{ApiOwner, MemberSpec};
use crate::units::script_view::{UnitRow, View};

/// A unit as a script holds it, `Unit` in scripts: its values as the view read them.
#[derive(Debug, Clone)]
pub(crate) struct Unit {
    pub(crate) id: StableId,
    view: View,
}

/// `unit.params`: the params of the unit's type, by name. `unit.params.aggro_range` reads
/// through the indexer, as Rhai tries one for a property with no getter.
#[derive(Debug, Clone)]
pub(crate) struct UnitParams(Unit);

impl Unit {
    /// Only the view makes a handle, of a unit it read: `View::unit` for any other code.
    pub(super) const fn new(id: StableId, view: View) -> Unit {
        Unit { id, view }
    }

    pub(crate) const fn view(&self) -> &View {
        &self.view
    }

    /// The unit's place among its view's rows.
    pub(crate) fn row_index(&self) -> usize {
        self.view
            .row_index(self.id)
            .expect("a handle's unit is in its view")
    }

    /// The unit's row. A handle comes only from its view's rows, and lives only within a call.
    pub(crate) fn row(&self) -> UnitRow {
        self.view
            .row(self.id)
            .expect("a handle's unit is in its view")
    }

    /// The `Unit` handle's fields and methods, and `Pos` with `distance_to` and `within`.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        Unit::register_fields(api);
        Unit::register_methods(api);
        Unit::register_positions(api);
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
            .bind(field("team", "its team's name"), |unit: &mut Unit| {
                unit.view.team_name(unit.row().team)
            })
            .bind(
                field("unit_type", "its unit type's name"),
                |unit: &mut Unit| unit.view.unit_type_name(&unit.row()),
            )
            .bind(
                field("path", "the name of the path it walks, `()` with none"),
                |unit: &mut Unit| unit.view.path_name(unit.row().path),
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
            method("can_see", "(unit)", "whether its team sees the other unit")
                .capability(Capability::Vision),
            |unit: &mut Unit, other: Unit| other.row().seen_by.contains(unit.row().team),
        )
        .bind(
            method(
                "recent_attackers",
                "(ms)",
                "the living units that struck it within the last `ms`, rounded up to whole ticks",
            )
            .capability(Capability::Combat),
            |unit: &mut Unit, ms: INT| unit.view.recent_attackers(unit, ms),
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

    fn register_positions(api: &mut ApiBuilder<'_>) {
        let position = |name, signature, description| {
            MemberSpec::method(ApiOwner::Position, name, signature, description)
        };
        let within = position(
            "within",
            "(pos, radius)",
            "whether `pos` is within `radius` in the map's metric, exactly: the test for reach",
        );
        api.ty::<Position>("Pos")
            .bind(
                position(
                    "distance_to",
                    "(pos)",
                    "the distance to `pos` in the map's metric",
                ),
                |call: NativeCallContext<'_>, from: &mut Position, to: Position| {
                    Ctx::of_call(&call)
                        .view()
                        .metric()
                        .offset(*from, to)
                        .checked_length()
                        .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
                },
            )
            .bind(
                within,
                |call: NativeCallContext<'_>, from: &mut Position, to: Position, radius: Num| {
                    Unit::within(&call, *from, to, radius)
                },
            )
            .bind(
                within,
                |call: NativeCallContext<'_>, from: &mut Position, to: Position, radius: INT| {
                    Unit::within(&call, *from, to, ApiError::num(radius)?)
                },
            )
            .bind(
                position(
                    "direction_to",
                    "(pos)",
                    "the unit vector towards `pos` in the map's metric, `()` for the same point",
                ),
                |call: NativeCallContext<'_>, from: &mut Position, to: Position| {
                    Ctx::of_call(&call)
                        .view()
                        .metric()
                        .offset(*from, to)
                        .normalized()
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            );
        let vector = |name, signature, description| {
            MemberSpec::method(ApiOwner::Vector, name, signature, description)
        };
        let rotated = vector(
            "rotated_deg",
            "(degrees)",
            "the vector turned by `degrees` about the vertical, counter-clockwise seen from above",
        );
        api.ty::<Vec3>("Vector")
            .bind(rotated, |vector: &mut Vec3, degrees: Num| {
                Unit::rotated(*vector, degrees)
            })
            .bind(rotated, |vector: &mut Vec3, degrees: INT| {
                Unit::rotated(*vector, ApiError::num(degrees)?)
            });
        api.plan(MemberSpec::operator(
            ApiOwner::Vector,
            "+",
            "the sum of two vectors",
        ))
        .plan(MemberSpec::operator(
            ApiOwner::Vector,
            "-",
            "the difference of two vectors",
        ))
        .plan(MemberSpec::operator(
            ApiOwner::Vector,
            "*",
            "the vector scaled by a number",
        ));
    }

    /// `vector` turned by `degrees` about the vertical; a turn too large to compute fails the call.
    fn rotated(vector: Vec3, degrees: Num) -> Checked<Vec3> {
        let radians = degrees
            .checked_mul(Num::PI)
            .and_then(|turn| turn.checked_div_int(180))
            .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))?;
        vector
            .checked_rotated_y(radians.sin_cos())
            .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
    }

    /// Whether `to` is within `radius` of `from` in the map's metric, exactly: as every range and
    /// query radius, so a script's reach agrees with combat's.
    fn within(
        call: &NativeCallContext<'_>,
        from: Position,
        to: Position,
        radius: Num,
    ) -> Checked<bool> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        Ok(Ctx::of_call(call).view().metric().within(from, to, radius))
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
