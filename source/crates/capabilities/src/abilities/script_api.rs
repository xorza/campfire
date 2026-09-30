use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use campfire_math::Num;
use campfire_script::rhai::{Dynamic, Engine, INT, ImmutableString};

use crate::abilities::frame::{Effect, Frame};
use crate::combat::damage_kind::DamageKind;
use crate::units::error::{ApiError, Checked};
use crate::units::script_view::View;
use crate::units::unit::Unit;

/// `ctx` in an ability's script: what one call reads, and the effects it queues. Effects apply
/// only after the call returns successfully, in the order queued, so a failed call changes
/// nothing.
#[derive(Debug, Clone)]
pub(crate) struct Ctx {
    frame: Rc<RefCell<Frame>>,
    view: View,
}

/// `ctx.p`: the running cast's params, by name. `ctx.p.damage` reads through the indexer, as
/// Rhai tries one for a property with no getter.
#[derive(Debug, Clone)]
pub(crate) struct Params(Ctx);

impl Ctx {
    pub(crate) fn new(view: View) -> Ctx {
        Ctx {
            frame: Rc::default(),
            view,
        }
    }

    /// The frame, borrowed until the guard drops. A call borrows it again, so no guard may live
    /// across a call.
    pub(crate) fn frame(&self) -> RefMut<'_, Frame> {
        self.frame.borrow_mut()
    }

    pub(crate) const fn view(&self) -> &View {
        &self.view
    }

    /// The script API of `on_cast`: `ctx.p`, the queries and `ctx.damage`.
    pub(crate) fn register(engine: &mut Engine) {
        engine
            .register_type_with_name::<Params>("Params")
            .register_indexer_get(|params: &mut Params, name: ImmutableString| params.get(&name));
        engine
            .register_type_with_name::<Ctx>("Ctx")
            .register_get("p", |ctx: &mut Ctx| Params(ctx.clone()))
            .register_fn(
                "damage",
                |ctx: &mut Ctx, target: Unit, amount: Num, kind: &str| {
                    ctx.damage(&target, amount, kind)
                },
            )
            .register_fn(
                "damage",
                |ctx: &mut Ctx, target: Unit, amount: INT, kind: &str| {
                    ctx.damage(&target, ApiError::num(amount)?, kind)
                },
            );
        View::register_queries::<Ctx>(engine, Ctx::view);
    }

    /// Queues `amount` of `kind` damage to `target`. The kind is checked; until the mode's
    /// `calc_damage` runs, it does not change the amount.
    fn damage(&self, target: &Unit, amount: Num, kind: &str) -> Checked<()> {
        DamageKind::parse(kind).ok_or_else(|| ApiError::UnknownDamageKind.fail())?;
        if amount < Num::ZERO {
            return Err(ApiError::NegativeDamage.fail().into());
        }
        self.frame().effects.push(Effect::Damage {
            target: target.id,
            amount,
        });
        Ok(())
    }
}

impl Params {
    /// The param `name`; one the ability does not declare fails the call.
    fn get(&self, name: &str) -> Checked<Dynamic> {
        let value = self.0.frame().param(name);
        Ok(value
            .ok_or_else(|| ApiError::UnknownParam.fail())?
            .to_dynamic())
    }
}
