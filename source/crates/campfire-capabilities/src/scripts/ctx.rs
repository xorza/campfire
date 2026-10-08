use std::any::Any;
use std::cell::{OnceCell, RefCell, RefMut};
use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_script::rhai::{Dynamic, NativeCallContext};
use campfire_sim::StableId;

use crate::scripts::effects::Effect;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::frame::Frame;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_role::ScriptRole;
use crate::units::script_view::View;

/// `ctx` in every script: what one call reads, and the effects it queues. Every role's call goes
/// through it, its frame saying whose call it is. Effects apply only after the call returns
/// successfully, in the order queued, so a failed call changes nothing.
#[derive(Debug, Clone)]
pub(crate) struct Ctx {
    frame: Rc<RefCell<Frame>>,
    view: View,
    /// The match's mode, once it installs, which only the mode reads as its own type.
    mode: Rc<OnceCell<Rc<dyn Any>>>,
}

/// `ctx.p`: the running call's params, by name: an ability's, a modifier's then its ability's,
/// or the mode's for a mode or AI call. `ctx.p.damage` reads through the indexer, as Rhai tries
/// one for a property with no getter.
#[derive(Debug, Clone)]
pub(crate) struct Params(pub(crate) Ctx);

impl Ctx {
    pub(crate) fn new(view: View) -> Ctx {
        Ctx {
            frame: Rc::default(),
            view,
            mode: Rc::default(),
        }
    }

    /// The `ctx` of the match whose script makes `call`: the host's tag.
    pub(crate) fn of_call(call: &NativeCallContext<'_>) -> Ctx {
        call.tag()
            .and_then(Dynamic::read_lock::<Ctx>)
            .expect("the units capability tags its host with the ctx")
            .clone()
    }

    /// The frame, borrowed until the guard drops. A call borrows it again, so no guard may live
    /// across a call.
    pub(crate) fn frame(&self) -> RefMut<'_, Frame> {
        self.frame.borrow_mut()
    }

    /// The frame to change, as `frame`; a pure hook's call fails.
    pub(crate) fn write(&self) -> Checked<RefMut<'_, Frame>> {
        let frame = self.frame();
        if frame.pure() {
            return Err(ApiError::PureCall.fail().into());
        }
        Ok(frame)
    }

    /// Fails the call unless its role is one of `roles`: a call given to some roles reads or
    /// writes what only their frames hold.
    pub(crate) fn require(&self, roles: RoleSet) -> Checked<()> {
        let role = self.frame().role();
        if role.is_none_or(|role| !roles.contains(role)) {
            return Err(ApiError::NotForRole.fail().into());
        }
        Ok(())
    }

    /// Queues `effect`; a pure hook's call fails.
    pub(crate) fn queue<E: Effect>(&self, effect: E) -> Checked<()> {
        self.write()?.effects.push(effect);
        Ok(())
    }

    pub(crate) const fn view(&self) -> &View {
        &self.view
    }

    /// The running call's acting unit.
    pub(crate) fn acting(&self) -> Option<StableId> {
        self.frame().acting()
    }

    /// Gives the match its mode.
    pub(crate) fn set_mode(&self, mode: Rc<dyn Any>) {
        assert!(self.mode.set(mode).is_ok(), "a match has one mode");
    }

    /// The match's mode, if it installed.
    pub(crate) fn mode(&self) -> Option<&dyn Any> {
        self.mode.get().map(|mode| &**mode)
    }

    /// Applies the effects of the call that ran, in tick `now`.
    pub(crate) fn apply(&self, world: &mut World, now: Tick) {
        self.frame().apply(world, now);
    }
}

impl Params {
    /// The param `name`; one the call's ability, modifier or mode does not declare fails it.
    pub(crate) fn get(&self, name: &str) -> Checked<Dynamic> {
        let ctx = &self.0;
        let role = ctx.frame().role();
        let modes = !matches!(role, Some(ScriptRole::Action | ScriptRole::Modifier));
        if modes && ctx.mode().is_none() {
            return Err(ApiError::NoMode.fail().into());
        }
        let value = ctx.frame().param_named(name);
        value.ok_or_else(|| ApiError::UnknownParam.fail().into())
    }
}
