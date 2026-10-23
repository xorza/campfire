use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_sim::StableId;
use rhai::{Dynamic, NativeCallContext};

use crate::script_host::ScriptHost;
use crate::scripts::api_holder::{ApiHolder, ScriptHandle};
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::frame::Frame;
use crate::scripts::frame::effects::{Effect, EffectKind};
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_role::ScriptRole;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::view::View;

/// `ctx` in every script: what one call reads, and the effects it queues. Every role's call goes
/// through it, its frame saying whose call it is. Effects apply only after the call returns
/// successfully, in the order queued, so a failed call changes nothing.
#[derive(Debug, Clone)]
pub struct Ctx(Rc<CtxShared>);

/// What every copy of a match's `ctx` shares.
#[derive(Debug)]
struct CtxShared {
    frame: RefCell<Frame>,
    view: View,
}

/// `ctx.params`: the running call's params, by name: an ability's, a modifier's then its ability's,
/// or the mode's for a mode or AI call. `ctx.params.damage` reads through the indexer, as Rhai
/// tries one for a property with no getter.
#[derive(Debug, Clone)]
pub struct Params(pub(crate) Ctx);

impl Ctx {
    pub fn new(view: View) -> Self {
        Self(Rc::new(CtxShared {
            frame: RefCell::default(),
            view,
        }))
    }

    /// Tags `host` with the ctx, which `of_call` reads in each call of its scripts, and counts
    /// the native work of its view's lists toward the host's calls.
    pub fn tag(&self, host: &mut ScriptHost) {
        host.share_charge(self.view().charge().clone());
        host.engine_mut()
            .set_default_tag(Dynamic::from(self.clone()));
    }

    /// The `ctx` of the match whose script makes `call`: the host's tag.
    pub(super) fn of_call(call: &NativeCallContext<'_>) -> Self {
        call.tag()
            .and_then(Dynamic::read_lock::<Self>)
            .expect("the host is tagged with the ctx")
            .clone()
    }

    /// The frame, borrowed until the guard drops. A call borrows it again, so no guard may live
    /// across a call.
    pub fn frame(&self) -> RefMut<'_, Frame> {
        self.0.frame.borrow_mut()
    }

    /// The frame to change the match through, as `frame`; a pure hook's call fails, and so does
    /// an AI's, which reads, orders and draws (06-script-api.md, An AI reads and orders), and a
    /// bot's, which runs outside the sim.
    pub fn write(&self) -> Checked<RefMut<'_, Frame>> {
        if matches!(self.frame().role(), Some(ScriptRole::Ai | ScriptRole::Bot)) {
            return Err(ApiError::NotForRole.fail().into());
        }
        self.drawing()
    }

    /// The frame to draw from, or to give an order through, as `frame`; a pure hook's call fails.
    pub(crate) fn drawing(&self) -> Checked<RefMut<'_, Frame>> {
        let frame = self.frame();
        if frame.is_pure() {
            return Err(ApiError::PureCall.fail().into());
        }
        Ok(frame)
    }

    /// Fails a call of a name for `roles` that its role may not make: one for every role passes,
    /// one for the sim's fails a bot's call alone, as a call outside any role, a test's probe,
    /// makes those, and one for some roles fails any other's.
    pub(crate) fn admit(&self, roles: RoleSet) -> Checked<()> {
        if roles == RoleSet::ALL {
            return Ok(());
        }
        if roles == RoleSet::SIM {
            if self.frame().role() == Some(ScriptRole::Bot) {
                return Err(ApiError::NotForRole.fail().into());
            }
            return Ok(());
        }
        self.require(roles)
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

    /// Queues `effect`; a pure hook's call fails, and so does an AI's for a change.
    pub fn queue<E: Effect>(&self, effect: E) -> Checked<()> {
        let mut frame = match E::KIND {
            EffectKind::Change => self.write()?,
            EffectKind::Order => self.drawing()?,
        };
        frame.queue(effect);
        Ok(())
    }

    pub fn view(&self) -> &View {
        &self.0.view
    }

    /// The running call's acting unit.
    pub fn acting(&self) -> Option<StableId> {
        self.frame().acting()
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

/// The holder of the calls and values of `ctx`.
impl ScriptHandle for Ctx {
    const NAME: &'static str = ApiHolder::CTX.name();
    const TITLE: &'static str = ApiHolder::CTX.title();
}

impl ScriptHandle for Params {
    const NAME: &'static str = "Params";
    const TITLE: &'static str = "`ctx.params`";
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::scripts::call_start::CallStart;
    use crate::scripts::ctx::Ctx;
    use crate::scripts::error::CallError;
    use crate::scripts::view::call_sight::CallSight;

    impl Ctx {
        /// Starts the call `start` in `world` alone, as a test's probe makes one: in a batch of
        /// its own, as `Frame::begin` says, read by the viewer of its role until the next call
        /// begins.
        pub fn begin_alone(&self, world: &World, start: CallStart) -> Result<(), CallError> {
            self.view().set_sight(CallSight::of(&start));
            let mut frame = self.frame();
            frame.begin_batch(world);
            frame.begin(world, &start)
        }
    }
}
