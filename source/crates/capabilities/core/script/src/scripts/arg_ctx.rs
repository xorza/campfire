use std::cell::OnceCell;

use rhai::NativeCallContext;

use crate::scripts::ctx::Ctx;

/// The `ctx` of the call an argument is given in, as `ScriptArg::take` reads it: the call's own,
/// or found from Rhai's context of a handle's method the first time an argument reads it, so a
/// method whose arguments name nothing finds none.
#[derive(Debug)]
pub enum ArgCtx<'a> {
    Ctx(&'a Ctx),
    Call(&'a NativeCallContext<'a>, OnceCell<Ctx>),
}

impl<'a> ArgCtx<'a> {
    /// The context of the method call `call`, found when an argument reads it.
    pub(super) const fn of_call(call: &'a NativeCallContext<'a>) -> Self {
        ArgCtx::Call(call, OnceCell::new())
    }

    pub fn get(&self) -> &Ctx {
        match self {
            ArgCtx::Ctx(ctx) => ctx,
            ArgCtx::Call(call, ctx) => ctx.get_or_init(|| Ctx::of_call(call)),
        }
    }
}
