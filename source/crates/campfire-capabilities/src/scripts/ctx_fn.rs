use campfire_script::rhai::{Engine, Variant};

use crate::scripts::ctx::Ctx;
use crate::scripts::error::Checked;
use crate::scripts::role_set::RoleSet;

/// A form of a member that takes `ctx` first, which `ApiBuilder::bind_for` binds for some roles
/// only: Rhai calls it as itself, and a call in another role fails before it runs.
pub(crate) trait CtxFn<Args, R> {
    fn register(self, engine: &mut Engine, name: String, roles: RoleSet);
}

// Rhai binds one native function type for each arity; so does this, up to the most arguments a
// member takes.
macro_rules! ctx_fn {
    ($($arg:ident),*) => {
        impl<F, R, $($arg),*> CtxFn<($($arg,)*), R> for F
        where
            F: Fn(&mut Ctx, $($arg),*) -> Checked<R> + 'static,
            R: Variant + Clone,
            $($arg: Variant + Clone,)*
        {
            #[allow(
                non_snake_case,
                reason = "each argument takes its type parameter's name; `expect` fails the arity with none"
            )]
            fn register(self, engine: &mut Engine, name: String, roles: RoleSet) {
                engine.register_fn(name, move |ctx: &mut Ctx, $($arg: $arg),*| -> Checked<R> {
                    ctx.require(roles)?;
                    self(ctx, $($arg),*)
                });
            }
        }
    };
}

ctx_fn!();
ctx_fn!(A);
ctx_fn!(A, B);
ctx_fn!(A, B, C);
ctx_fn!(A, B, C, D);
ctx_fn!(A, B, C, D, E);
