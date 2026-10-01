use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use campfire_math::Num;
use campfire_script::rhai::{Dynamic, Engine, INT, ImmutableString};

use campfire_sim::Ticks;

use crate::abilities::frame::{Effect, Frame};
use crate::scripts::error::{ApiError, Checked};
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifier_handle::ModifierHandle;
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

    /// The script API of `on_cast` and of modifier hooks: `ctx.p`, the queries, `ctx.damage`,
    /// `ctx.heal`, `ctx.restore`, `ctx.attack_hit`, `ctx.add_modifier` and `ctx.remove`.
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
            )
            .register_fn("heal", |ctx: &mut Ctx, unit: Unit, amount: Num| {
                ctx.heal(&unit, amount)
            })
            .register_fn("heal", |ctx: &mut Ctx, unit: Unit, amount: INT| {
                ctx.heal(&unit, ApiError::num(amount)?)
            })
            .register_fn("restore", |ctx: &mut Ctx, unit: Unit, amount: Num| {
                ctx.restore(&unit, amount)
            })
            .register_fn("restore", |ctx: &mut Ctx, unit: Unit, amount: INT| {
                ctx.restore(&unit, ApiError::num(amount)?)
            })
            .register_fn("attack_hit", |ctx: &mut Ctx, target: Unit| {
                ctx.attack_hit(&target)
            })
            .register_fn("add_modifier", |ctx: &mut Ctx, target: Unit, id: &str| {
                ctx.add_modifier(&target, id, None)
            })
            .register_fn(
                "add_modifier",
                |ctx: &mut Ctx, target: Unit, id: &str, ms: INT| {
                    let ticks = ctx.view().ticks(ms)?;
                    ctx.add_modifier(&target, id, Some(ticks))
                },
            )
            .register_fn("remove", |ctx: &mut Ctx, handle: ModifierHandle| {
                ctx.frame().effects.push(Effect::Modifier(handle.remove()));
            });
        View::register_queries::<Ctx>(engine, Ctx::view);
    }

    /// Queues modifier `id` of the cast's package on `target`, from the caster, for `duration`
    /// when given, and gives its handle.
    fn add_modifier(
        &self,
        target: &Unit,
        id: &str,
        duration: Option<Ticks>,
    ) -> Checked<ModifierHandle> {
        let id = self.view().modifier(id)?;
        let mut frame = self.frame();
        let source = frame.caster;
        let handle = self
            .view()
            .applied_handle(&mut frame.handles, target.id, id, source);
        frame.effects.push(Effect::Modifier(ModifierEffect::Add {
            target: target.id,
            id,
            duration,
        }));
        Ok(handle)
    }

    /// Queues `amount` of `kind` damage to `target`, a kind the mode declares, which the damage
    /// pass deals.
    fn damage(&self, target: &Unit, amount: Num, kind: &str) -> Checked<()> {
        let kind = self.view().damage_kind(kind)?;
        if amount < Num::ZERO {
            return Err(ApiError::NegativeDamage.fail().into());
        }
        self.frame().effects.push(Effect::Damage {
            target: target.id,
            amount,
            kind,
        });
        Ok(())
    }

    /// Queues an extra attack of the acting unit on `target`, which has an attack.
    fn attack_hit(&self, target: &Unit) -> Checked<()> {
        let acting = self.frame().caster;
        let attacks = acting
            .and_then(|id| self.view().row(id))
            .is_some_and(|row| row.attack_range.is_some());
        if !attacks {
            return Err(ApiError::NoAttack.fail().into());
        }
        self.frame()
            .effects
            .push(Effect::AttackHit { target: target.id });
        Ok(())
    }

    /// Queues a heal of `amount` to `unit`, as `Combat::heal` deals it.
    fn heal(&self, unit: &Unit, amount: Num) -> Checked<()> {
        if amount < Num::ZERO {
            return Err(ApiError::NegativeHeal.fail().into());
        }
        self.frame().effects.push(Effect::Heal {
            unit: unit.id,
            amount,
        });
        Ok(())
    }

    /// Queues `amount` of `unit`'s resource back, as `Combat::restore` gives it.
    fn restore(&self, unit: &Unit, amount: Num) -> Checked<()> {
        if amount < Num::ZERO {
            return Err(ApiError::NegativeHeal.fail().into());
        }
        self.frame().effects.push(Effect::Restore {
            unit: unit.id,
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
