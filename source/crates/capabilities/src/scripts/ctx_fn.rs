use std::rc::Rc;

use campfire_script::rhai::{Engine, NativeCallContext, Variant};

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::Checked;
use crate::scripts::name_kind::NameKind;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::member_spec::{ArgKinds, MemberSpec};
use crate::scripts::script_arg::ScriptArg;
use crate::values::engine_enum::EngineEnum;

/// A form of a call or a value of `ctx`, which takes `ctx` first and then its arguments, each a
/// `ScriptArg`. Rhai calls it with each argument as the script gives it, and a second time, its
/// integer form, with an integer for each `Num`; a call in a role it does not serve fails before
/// it runs.
pub(crate) trait CtxFn<Args, R> {
    /// What each argument names, and the engine enum each takes, by place.
    fn args() -> ArgKinds;

    fn register(self, engine: &mut Engine, name: &str, roles: RoleSet);
}

/// A form of a method or an operator of a handle `H`, which takes the handle first and then its
/// arguments, each a `ScriptArg` as the handle is, and the call's `ctx` before both when its marker
/// `M` is `WithCtx`. Rhai calls it as `CtxFn` says, the handle a number too in the integer form.
pub(crate) trait MethodFn<M, H, Args, R> {
    /// What each argument names, and the engine enum each takes, by place.
    fn args() -> ArgKinds;

    fn register(self, engine: &mut Engine, name: &str);
}

/// The marker of a method that reads the call's `ctx`.
#[derive(Debug)]
pub(crate) struct WithCtx;

/// The marker of a method that reads only its handle and its arguments.
#[derive(Debug)]
pub(crate) struct WithoutCtx;

/// What arguments that name `names` and take `enums` name and take, by place, among the
/// arguments a spec records.
fn args_of(names: &[Option<NameKind>], enums: &[Option<EngineEnum>]) -> ArgKinds {
    let recorded =
        |at: usize| at < MemberSpec::ARGS || (names[at].is_none() && enums[at].is_none());
    assert!(
        (0..names.len()).all(recorded),
        "what an argument names is among the arguments a spec records"
    );
    let mut args = ArgKinds {
        names: [None; MemberSpec::ARGS],
        enums: [None; MemberSpec::ARGS],
    };
    for (at, (&name, &taken)) in names.iter().zip(enums).enumerate().take(MemberSpec::ARGS) {
        args.names[at] = name;
        args.enums[at] = taken;
    }
    args
}

// Rhai binds one native function type for each arity; so do these, up to the most arguments a
// member takes.
macro_rules! script_fn {
    ($($arg:ident),*) => {
        impl<F, R, $($arg),*> CtxFn<($($arg,)*), R> for F
        where
            F: Fn(&mut Ctx, $($arg),*) -> Checked<R> + 'static,
            R: Variant + Clone,
            $($arg: ScriptArg,)*
        {
            fn args() -> ArgKinds {
                args_of(&[$($arg::NAMES),*], &[$($arg::ENUM),*])
            }

            #[expect(
                clippy::allow_attributes,
                reason = "`expect` fails the arities whose expansion gives no case of the lints below"
            )]
            #[allow(
                non_snake_case,
                unused_variables,
                reason = "each argument takes its type parameter's name; `expect` fails the arity with none, and with no argument none reads the context"
            )]
            fn register(self, engine: &mut Engine, name: &str, roles: RoleSet) {
                let each = Rc::new(self);
                let int = Rc::clone(&each);
                engine.register_fn(name, move |ctx: &mut Ctx, $($arg: $arg::Raw),*| -> Checked<R> {
                    if roles != RoleSet::ALL {
                        ctx.require(roles)?;
                    }
                    let args = ArgCtx::Ctx(ctx);
                    $(let $arg = $arg::take($arg, &args)?;)*
                    each(ctx, $($arg),*)
                });
                if [$($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(name, move |ctx: &mut Ctx, $($arg: $arg::Int),*| -> Checked<R> {
                        if roles != RoleSet::ALL {
                            ctx.require(roles)?;
                        }
                        let args = ArgCtx::Ctx(ctx);
                        $(let $arg = $arg::take_int($arg, &args)?;)*
                        int(ctx, $($arg),*)
                    });
                }
            }
        }

        impl<F, R, H, $($arg),*> MethodFn<WithoutCtx, H, ($($arg,)*), R> for F
        where
            F: Fn(H, $($arg),*) -> Checked<R> + 'static,
            R: Variant + Clone,
            H: ScriptArg,
            $($arg: ScriptArg,)*
        {
            fn args() -> ArgKinds {
                args_of(&[$($arg::NAMES),*], &[$($arg::ENUM),*])
            }

            #[expect(
                clippy::allow_attributes,
                reason = "`expect` fails the arities whose expansion gives no case of the lints below"
            )]
            #[allow(
                non_snake_case,
                reason = "each argument takes its type parameter's name; `expect` fails the arity with none"
            )]
            fn register(self, engine: &mut Engine, name: &str) {
                let each = Rc::new(self);
                let int = Rc::clone(&each);
                engine.register_fn(
                    name,
                    move |call: NativeCallContext<'_>, handle: H::Raw, $($arg: $arg::Raw),*| -> Checked<R> {
                        let args = ArgCtx::of_call(&call);
                        let handle = H::take(handle, &args)?;
                        $(let $arg = $arg::take($arg, &args)?;)*
                        each(handle, $($arg),*)
                    },
                );
                if [H::NUMBER, $($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(
                        name,
                        move |call: NativeCallContext<'_>, handle: H::Int, $($arg: $arg::Int),*| -> Checked<R> {
                            let args = ArgCtx::of_call(&call);
                            let handle = H::take_int(handle, &args)?;
                            $(let $arg = $arg::take_int($arg, &args)?;)*
                            int(handle, $($arg),*)
                        },
                    );
                }
            }
        }

        impl<F, R, H, $($arg),*> MethodFn<WithCtx, H, ($($arg,)*), R> for F
        where
            F: Fn(&Ctx, H, $($arg),*) -> Checked<R> + 'static,
            R: Variant + Clone,
            H: ScriptArg,
            $($arg: ScriptArg,)*
        {
            fn args() -> ArgKinds {
                args_of(&[$($arg::NAMES),*], &[$($arg::ENUM),*])
            }

            #[expect(
                clippy::allow_attributes,
                reason = "`expect` fails the arities whose expansion gives no case of the lints below"
            )]
            #[allow(
                non_snake_case,
                reason = "each argument takes its type parameter's name; `expect` fails the arity with none"
            )]
            fn register(self, engine: &mut Engine, name: &str) {
                let each = Rc::new(self);
                let int = Rc::clone(&each);
                engine.register_fn(
                    name,
                    move |call: NativeCallContext<'_>, handle: H::Raw, $($arg: $arg::Raw),*| -> Checked<R> {
                        let args = ArgCtx::of_call(&call);
                        let handle = H::take(handle, &args)?;
                        $(let $arg = $arg::take($arg, &args)?;)*
                        each(args.get(), handle, $($arg),*)
                    },
                );
                if [H::NUMBER, $($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(
                        name,
                        move |call: NativeCallContext<'_>, handle: H::Int, $($arg: $arg::Int),*| -> Checked<R> {
                            let args = ArgCtx::of_call(&call);
                            let handle = H::take_int(handle, &args)?;
                            $(let $arg = $arg::take_int($arg, &args)?;)*
                            int(args.get(), handle, $($arg),*)
                        },
                    );
                }
            }
        }
    };
}

script_fn!();
script_fn!(A);
script_fn!(A, B);
script_fn!(A, B, C);
script_fn!(A, B, C, D);
script_fn!(A, B, C, D, E);
