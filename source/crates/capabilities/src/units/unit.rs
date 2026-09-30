use campfire_math::Num;
use campfire_script::NumError;
use campfire_script::Raised;
use campfire_script::rhai::{Dynamic, Engine, INT, ImmutableString};
use campfire_sim::{Position, StableId};

use crate::scripts::error::{ApiError, Checked};
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
    pub(crate) const fn new(id: StableId, view: View) -> Unit {
        Unit { id, view }
    }

    /// The unit's row. A handle comes only from its view's rows, and lives only within a call.
    pub(crate) fn row(&self) -> UnitRow {
        self.view
            .row(self.id)
            .expect("a handle's unit is in its view")
    }

    /// The core script API of units and positions: the `Unit` handle's fields, and `Pos` with
    /// `distance_to` and `within`.
    pub(crate) fn register(engine: &mut Engine) {
        engine
            .register_type_with_name::<Unit>("Unit")
            .register_get("pos", |unit: &mut Unit| unit.row().pos)
            .register_get("alive", |unit: &mut Unit| unit.row().alive)
            .register_get("is_hero", |unit: &mut Unit| unit.view.is_hero(&unit.row()))
            .register_get("target", |unit: &mut Unit| unit.target())
            .register_get("attack_range", |unit: &mut Unit| -> Checked<Num> {
                unit.row()
                    .attack_range
                    .ok_or_else(|| ApiError::NoAttack.fail().into())
            })
            .register_get("params", |unit: &mut Unit| UnitParams(unit.clone()))
            .register_get("team", |unit: &mut Unit| {
                unit.view.team_name(unit.row().team)
            })
            .register_get("unit_type", |unit: &mut Unit| {
                unit.view.unit_type_name(&unit.row())
            })
            .register_get("lane", |unit: &mut Unit| {
                unit.view.lane_name(unit.row().lane)
            })
            .register_get("owner", |unit: &mut Unit| {
                unit.row().owner.map_or(Dynamic::UNIT, |slot| {
                    Dynamic::from_int(INT::from(slot.get()))
                })
            })
            .register_fn("has_tag", |unit: &mut Unit, name: &str| -> Checked<bool> {
                let tag = unit.view.tag(name).map_err(ApiError::fail)?;
                Ok(unit.view.has_tag(&unit.row(), tag))
            })
            .register_fn("is_enemy_of", |unit: &mut Unit, other: Unit| {
                unit.row().team.is_enemy_of(other.row().team)
            })
            .register_fn("can_see", |_: &mut Unit, _: Unit| true)
            .register_fn("recent_attackers", |unit: &mut Unit, ms: INT| {
                unit.view.recent_attackers(unit, ms)
            })
            .register_fn("==", |a: Unit, b: Unit| a.id == b.id)
            .register_fn("!=", |a: Unit, b: Unit| a.id != b.id);
        engine
            .register_type_with_name::<UnitParams>("UnitParams")
            .register_indexer_get(|params: &mut UnitParams, name: ImmutableString| {
                params.get(&name)
            });
        engine
            .register_type_with_name::<Position>("Pos")
            .register_fn("distance_to", |from: &mut Position, to: Position| {
                from.get()
                    .checked_distance(to.get())
                    .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
            })
            .register_fn(
                "within",
                |from: &mut Position, to: Position, radius: Num| Unit::within(*from, to, radius),
            )
            .register_fn(
                "within",
                |from: &mut Position, to: Position, radius: INT| {
                    Unit::within(*from, to, ApiError::num(radius)?)
                },
            );
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
