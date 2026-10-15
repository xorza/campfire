use std::any::Any;
use std::cell::{OnceCell, RefCell, RefMut};
use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_sim::StableId;
use rhai::{Dynamic, NativeCallContext};

use crate::script_host::ScriptHost;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::frame::Frame;
use crate::scripts::frame::effects::Effect;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::view::View;

/// `ctx` in every script: what one call reads, and the effects it queues. Every role's call goes
/// through it, its frame saying whose call it is. Effects apply only after the call returns
/// successfully, in the order queued, so a failed call changes nothing.
#[derive(Debug, Clone)]
pub struct Ctx(Rc<CtxParts>);

/// What every copy of a match's `ctx` shares.
#[derive(Debug)]
struct CtxParts {
    frame: RefCell<Frame>,
    view: View,
    /// The match's mode, once it installs, which only the mode reads as its own type.
    mode: OnceCell<Rc<dyn Any>>,
}

/// `ctx.p`: the running call's params, by name: an ability's, a modifier's then its ability's,
/// or the mode's for a mode or AI call. `ctx.p.damage` reads through the indexer, as Rhai tries
/// one for a property with no getter.
#[derive(Debug, Clone)]
pub struct Params(pub(crate) Ctx);

impl Ctx {
    pub fn new(view: View) -> Ctx {
        Ctx(Rc::new(CtxParts {
            frame: RefCell::default(),
            view,
            mode: OnceCell::new(),
        }))
    }

    /// Tags `host` with the ctx, which `of_call` reads in each call of its scripts.
    pub fn tag(&self, host: &mut ScriptHost) {
        host.engine_mut()
            .set_default_tag(Dynamic::from(self.clone()));
    }

    /// The `ctx` of the match whose script makes `call`: the host's tag.
    pub(super) fn of_call(call: &NativeCallContext<'_>) -> Ctx {
        call.tag()
            .and_then(Dynamic::read_lock::<Ctx>)
            .expect("the host is tagged with the ctx")
            .clone()
    }

    /// The frame, borrowed until the guard drops. A call borrows it again, so no guard may live
    /// across a call.
    pub fn frame(&self) -> RefMut<'_, Frame> {
        self.0.frame.borrow_mut()
    }

    /// The frame to change, as `frame`; a pure hook's call fails.
    pub fn write(&self) -> Checked<RefMut<'_, Frame>> {
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
    pub fn queue<E: Effect>(&self, effect: E) -> Checked<()> {
        self.write()?.effects.push(effect);
        Ok(())
    }

    pub fn view(&self) -> &View {
        &self.0.view
    }

    /// The running call's acting unit.
    pub fn acting(&self) -> Option<StableId> {
        self.frame().acting()
    }

    /// Gives the match its mode.
    pub fn set_mode(&self, mode: Rc<dyn Any>) {
        assert!(self.0.mode.set(mode).is_ok(), "a match has one mode");
    }

    /// The match's mode, if it installed.
    pub fn mode(&self) -> Option<&dyn Any> {
        self.0.mode.get().map(|mode| &**mode)
    }

    /// Applies the effects of the call that ran, in its tick.
    pub fn apply(&self, world: &mut World) {
        self.frame().apply(world);
    }
}

impl Params {
    /// The param `name`; one the call's ability, modifier or mode does not declare fails it.
    pub fn get(&self, name: &str) -> Checked<ScriptValue> {
        let value = self.0.frame().param(name);
        value.map_err(|error| error.fail().into())
    }
}
