use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use campfire_math::{Num, Vec3};
use campfire_script::Raised;
use campfire_script::rhai::{Array, Dynamic, Engine, EvalAltResult, INT, ImmutableString};
use campfire_sim::Position;

use crate::abilities::ability_data::{Relation, Scalar};
use crate::abilities::error::ApiError;
use crate::abilities::frame::{Effect, Frame};
use crate::combat::attack_stats::ground_offset;
use crate::combat::damage_kind::DamageKind;
use crate::combat::living_unit::LivingUnit;

type Checked<T> = Result<T, Box<EvalAltResult>>;

/// `ctx` in a script: what one call reads, and the effects it queues. Effects apply only after
/// the call returns successfully, in the order queued, so a failed call changes nothing.
#[derive(Debug, Clone, Default)]
pub(crate) struct Ctx(Rc<RefCell<Frame>>);

/// `ctx.p`: the running cast's params, by name. `ctx.p.damage` reads through the indexer, as
/// Rhai tries one for a property with no getter.
#[derive(Debug, Clone)]
pub(crate) struct Params(Ctx);

impl Ctx {
    /// The frame, borrowed until the guard drops. A call borrows it again, so no guard may live
    /// across a call.
    pub(crate) fn frame(&self) -> RefMut<'_, Frame> {
        self.0.borrow_mut()
    }

    /// The script API of `on_cast`: `ctx.p`, `ctx.find`, `ctx.damage`, and the `Unit` and `Pos`
    /// handles.
    pub(crate) fn register(engine: &mut Engine) {
        engine
            .register_type_with_name::<Params>("Params")
            .register_indexer_get(|params: &mut Params, name: ImmutableString| params.get(&name));
        engine
            .register_type_with_name::<Ctx>("Ctx")
            .register_get("p", |ctx: &mut Ctx| Params(ctx.clone()))
            .register_fn(
                "find",
                |ctx: &mut Ctx, of: LivingUnit, pos: Position, radius: Num, filter: &str| {
                    ctx.find(of, pos, radius, filter)
                },
            )
            .register_fn(
                "find",
                |ctx: &mut Ctx, of: LivingUnit, pos: Position, radius: INT, filter: &str| {
                    ctx.find(of, pos, int(radius)?, filter)
                },
            )
            .register_fn(
                "damage",
                |ctx: &mut Ctx, target: LivingUnit, amount: Num, kind: &str| {
                    ctx.damage(target, amount, kind)
                },
            )
            .register_fn(
                "damage",
                |ctx: &mut Ctx, target: LivingUnit, amount: INT, kind: &str| {
                    ctx.damage(target, int(amount)?, kind)
                },
            );
        engine
            .register_type_with_name::<LivingUnit>("Unit")
            .register_get("pos", |unit: &mut LivingUnit| unit.pos)
            .register_fn("==", |a: LivingUnit, b: LivingUnit| a.id == b.id)
            .register_fn("!=", |a: LivingUnit, b: LivingUnit| a.id != b.id);
        engine.register_type_with_name::<Position>("Pos");
    }

    /// The living units within `radius` of `pos` on the ground plane that `filter` selects
    /// relative to `of`, by stable id.
    fn find(&self, of: LivingUnit, pos: Position, radius: Num, filter: &str) -> Checked<Array> {
        let relation =
            Relation::parse(filter).ok_or_else(|| Raised::error(ApiError::UnknownFilter))?;
        if radius < Num::ZERO {
            return Err(Raised::error(ApiError::NegativeRadius).into());
        }
        let frame = self.0.borrow();
        Ok(frame
            .units
            .iter()
            .filter(|unit| relation.holds(of.team, unit.team))
            .filter(|unit| Vec3::ZERO.within(ground_offset(pos, unit.pos), radius))
            .map(|&unit| Dynamic::from(unit))
            .collect())
    }

    /// Queues `amount` of `kind` damage to `target`. The kind is checked; until the mode's
    /// `calc_damage` runs, it does not change the amount.
    fn damage(&self, target: LivingUnit, amount: Num, kind: &str) -> Checked<()> {
        DamageKind::parse(kind).ok_or_else(|| Raised::error(ApiError::UnknownDamageKind))?;
        if amount < Num::ZERO {
            return Err(Raised::error(ApiError::NegativeDamage).into());
        }
        self.frame().effects.push(Effect::Damage {
            target: target.id,
            amount,
        });
        Ok(())
    }
}

fn int(value: INT) -> Checked<Num> {
    Num::from_int(value).ok_or_else(|| Raised::error(ApiError::IntegerBeyondNum).into())
}

impl Params {
    /// The param `name`; one the ability does not declare fails the call.
    fn get(&self, name: &str) -> Checked<Dynamic> {
        let value = self.0.frame().param(name);
        match value.ok_or_else(|| Raised::error(ApiError::UnknownParam))? {
            Scalar::Int(value) => Ok(Dynamic::from_int(value)),
            Scalar::Decimal(value) => Ok(Dynamic::from(value)),
        }
    }
}
