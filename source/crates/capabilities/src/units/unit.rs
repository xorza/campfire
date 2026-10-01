use campfire_math::Num;
use campfire_script::NumError;
use campfire_script::Raised;
use campfire_script::rhai::{Dynamic, INT, ImmutableString};
use campfire_sim::{Capability, Position, StableId};

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::error::{ApiError, Checked};
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
                |unit: &mut Unit| unit.view.is_avatar(&unit.row()),
            )
            .bind(
                field("target", "its attack's target, `()` with none"),
                |unit: &mut Unit| unit.target(),
            )
            .bind(
                field("attack_range", "its attack's range").capability(Capability::Combat),
                |unit: &mut Unit| -> Checked<Num> {
                    unit.row()
                        .attack_range
                        .ok_or_else(|| ApiError::NoAttack.fail().into())
                },
            )
            .bind(
                field("params", "its unit type's params, unresolved"),
                |unit: &mut Unit| UnitParams(unit.clone()),
            )
            .bind(
                field("level", "its level").capability(Capability::Stats),
                |unit: &mut Unit| -> Checked<INT> {
                    let level = unit.row().level.ok_or_else(|| ApiError::NoStats.fail())?;
                    Ok(INT::from(level))
                },
            )
            .bind(
                field("health", "its health").capability(Capability::Combat),
                |unit: &mut Unit| -> Checked<Num> {
                    let health = unit.row().health.ok_or_else(|| ApiError::NoHealth.fail())?;
                    Ok(health.current())
                },
            )
            .bind(
                field("max_health", "its health's maximum").capability(Capability::Combat),
                |unit: &mut Unit| -> Checked<Num> {
                    let health = unit.row().health.ok_or_else(|| ApiError::NoHealth.fail())?;
                    Ok(health.max())
                },
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
            .plan(field("spawn_pos", "where it spawned"));
        api.ty::<UnitParams>("UnitParams")
            .index(|params: &mut UnitParams, name: ImmutableString| params.get(&name));
    }

    fn register_methods(api: &mut ApiBuilder<'_>) {
        let method = |name, signature, description| {
            MemberSpec::method(ApiOwner::Unit, name, signature, description)
        };
        api.bind(
            method("stat", "(name)", "its value of a stat the mode declares")
                .capability(Capability::Stats),
            |unit: &mut Unit, name: &str| unit.view.stat(&unit.row(), name),
        )
        .bind(
            method("has_tag", "(tag)", "whether its unit type has the tag"),
            |unit: &mut Unit, name: &str| -> Checked<bool> {
                let tag = unit.view.tag(name).map_err(ApiError::fail)?;
                Ok(unit.view.has_tag(&unit.row(), tag))
            },
        )
        .bind(
            method(
                "has_modifier",
                "(id)",
                "whether it carries the modifier of the script's package",
            )
            .capability(Capability::Stats),
            |unit: &mut Unit, id: &str| unit.view.has_modifier(&unit.row(), id),
        )
        .bind(
            method(
                "is_enemy_of",
                "(unit)",
                "whether the two are of enemy teams",
            ),
            |unit: &mut Unit, other: Unit| unit.row().team.is_enemy_of(other.row().team),
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
            "whether `pos` is within `radius` on the ground plane, exactly: the test for reach",
        );
        api.ty::<Position>("Pos")
            .bind(
                position("distance_to", "(pos)", "the distance to `pos`"),
                |from: &mut Position, to: Position| {
                    from.get()
                        .checked_distance(to.get())
                        .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
                },
            )
            .bind(within, |from: &mut Position, to: Position, radius: Num| {
                Unit::within(*from, to, radius)
            })
            .bind(within, |from: &mut Position, to: Position, radius: INT| {
                Unit::within(*from, to, ApiError::num(radius)?)
            })
            .plan(position(
                "direction_to",
                "(pos)",
                "the unit vector towards `pos`",
            ));
        let vector = |name, signature, description| {
            MemberSpec::method(ApiOwner::Vector, name, signature, description)
        };
        api.plan(vector(
            "rotated_deg",
            "(degrees)",
            "the vector turned by `degrees`",
        ))
        .plan(MemberSpec::operator(
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

    /// Whether `to` is within `radius` of `from` on the ground plane, exactly: as every range and
    /// query radius, so a script's reach agrees with combat's.
    fn within(from: Position, to: Position, radius: Num) -> Checked<bool> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        Ok(from.within_ground(to, radius))
    }

    /// The unit's attack target, `()` when it has none or the view did not read it.
    fn target(&self) -> Dynamic {
        self.row()
            .target
            .and_then(|target| self.view.unit(target))
            .map_or(Dynamic::UNIT, Dynamic::from)
    }
}

impl UnitParams {
    /// The param `name`; one the unit's type does not declare fails the call.
    fn get(&self, name: &str) -> Checked<Dynamic> {
        let unit = &self.0;
        unit.view
            .param(&unit.row(), name)
            .ok_or_else(|| ApiError::UnknownParam.fail().into())
    }
}
