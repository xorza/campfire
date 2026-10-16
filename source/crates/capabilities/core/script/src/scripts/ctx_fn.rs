use std::any::TypeId;
use std::rc::Rc;

use campfire_values::EngineEnum;
use rhai::{Dynamic, Engine, NativeCallContext};

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::Checked;
use crate::scripts::member_spec::{ArgKinds, MemberSpec};
use crate::scripts::name_kind::NameKind;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_arg::ScriptArg;
use crate::scripts::script_value::{ScriptType, ScriptValue};

/// A form of a call or a value of `ctx`, which takes `ctx` first and then its arguments, each a
/// `ScriptArg`. Rhai calls it with each argument as the script gives it, and a second time, its
/// integer form, with an integer for each `Num`; a call in a role it does not serve fails before
/// it runs.
pub trait CtxFn<Args, R> {
    /// How many arguments it takes, `ctx` aside.
    const ARITY: usize;

    /// What each argument names, and the engine enum each takes, by place.
    fn args() -> ArgKinds;

    /// The type of the value it gives.
    fn output() -> TypeId;

    fn register(self, engine: &mut Engine, name: &str, roles: RoleSet);
}

/// A form of a method or an operator of a handle `H`, which takes the handle first and then its
/// arguments, each a `ScriptArg` as the handle is, and the call's `ctx` before both when its marker
/// `M` is `WithCtx`. Rhai calls it as `CtxFn` says, the handle a number too in the integer form.
pub trait MethodFn<M, H, Args, R> {
    /// How many arguments it takes, the handle and `ctx` aside.
    const ARITY: usize;

    /// What each argument names, and the engine enum each takes, by place.
    fn args() -> ArgKinds;

    /// The type of the value it gives.
    fn output() -> TypeId;

    fn register(self, engine: &mut Engine, name: &str);
}

/// A form of a value of `ctx` or a field's getter of a handle `H`, which takes the handle, and the
/// call's `ctx` before it when its marker `M` is `WithCtx`. It gives its value `O`: a value, or
/// one that fails the call when its marker `K` is `Fallible`.
pub trait GetFn<M, H, O, K> {
    /// The type of the value it gives.
    fn output() -> TypeId;

    fn register(self, engine: &mut Engine, name: &str);
}

/// What a form gives: its value, or with the marker `Fallible`, its value or the error that
/// fails the call.
pub(crate) trait ScriptReturn<K> {
    /// The type of the value.
    type Value: 'static;

    /// The value in the engine's form, or the error.
    fn checked(self) -> Checked<Dynamic>;
}

/// The marker of a form that gives its value.
#[derive(Debug)]
pub struct Plain;

/// The marker of a form that may fail the call.
#[derive(Debug)]
pub struct Fallible;

impl<R: ScriptType> ScriptReturn<Plain> for R {
    type Value = R;

    fn checked(self) -> Checked<Dynamic> {
        Ok(ScriptValue::engine(self))
    }
}

impl<R: ScriptType> ScriptReturn<Fallible> for Checked<R> {
    type Value = R;

    fn checked(self) -> Checked<Dynamic> {
        self.map(ScriptValue::engine)
    }
}

impl<F, H, O, K> GetFn<WithoutCtx, H, O, K> for F
where
    F: Fn(&mut H) -> O + 'static,
    H: ScriptType,
    O: ScriptReturn<K>,
{
    fn output() -> TypeId {
        TypeId::of::<O::Value>()
    }

    fn register(self, engine: &mut Engine, name: &str) {
        engine.register_fn(name, move |handle: &mut H| self(handle).checked());
    }
}

impl<F, H, O, K> GetFn<WithCtx, H, O, K> for F
where
    F: Fn(&Ctx, &mut H) -> O + 'static,
    H: ScriptType,
    O: ScriptReturn<K>,
{
    fn output() -> TypeId {
        TypeId::of::<O::Value>()
    }

    fn register(self, engine: &mut Engine, name: &str) {
        engine.register_fn(name, move |call: NativeCallContext<'_>, handle: &mut H| {
            self(&Ctx::of_call(&call), handle).checked()
        });
    }
}

/// The marker of a method that reads the call's `ctx`.
#[derive(Debug)]
pub struct WithCtx;

/// The marker of a method that reads only its handle and its arguments.
#[derive(Debug)]
pub struct WithoutCtx;

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
            R: ScriptType,
            $($arg: ScriptArg,)*
        {
            const ARITY: usize = {
                let args: &[&str] = &[$(stringify!($arg)),*];
                args.len()
            };

            fn args() -> ArgKinds {
                args_of(&[$($arg::NAMES),*], &[$($arg::ENUM),*])
            }

            fn output() -> TypeId {
                TypeId::of::<R>()
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
                engine.register_fn(name, move |ctx: &mut Ctx, $($arg: $arg::Raw),*| -> Checked<Dynamic> {
                    if roles != RoleSet::ALL {
                        ctx.require(roles)?;
                    }
                    let args = ArgCtx::Ctx(ctx);
                    $(let $arg = $arg::take($arg, &args)?;)*
                    each(ctx, $($arg),*).map(ScriptValue::engine)
                });
                if [$($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(name, move |ctx: &mut Ctx, $($arg: $arg::Int),*| -> Checked<Dynamic> {
                        if roles != RoleSet::ALL {
                            ctx.require(roles)?;
                        }
                        let args = ArgCtx::Ctx(ctx);
                        $(let $arg = $arg::take_int($arg, &args)?;)*
                        int(ctx, $($arg),*).map(ScriptValue::engine)
                    });
                }
            }
        }

        impl<F, R, H, $($arg),*> MethodFn<WithoutCtx, H, ($($arg,)*), R> for F
        where
            F: Fn(H, $($arg),*) -> Checked<R> + 'static,
            R: ScriptType,
            H: ScriptArg,
            $($arg: ScriptArg,)*
        {
            const ARITY: usize = {
                let args: &[&str] = &[$(stringify!($arg)),*];
                args.len()
            };

            fn args() -> ArgKinds {
                args_of(&[$($arg::NAMES),*], &[$($arg::ENUM),*])
            }

            fn output() -> TypeId {
                TypeId::of::<R>()
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
                    move |call: NativeCallContext<'_>, handle: H::Raw, $($arg: $arg::Raw),*| -> Checked<Dynamic> {
                        let args = ArgCtx::of_call(&call);
                        let handle = H::take(handle, &args)?;
                        $(let $arg = $arg::take($arg, &args)?;)*
                        each(handle, $($arg),*).map(ScriptValue::engine)
                    },
                );
                if [H::NUMBER, $($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(
                        name,
                        move |call: NativeCallContext<'_>, handle: H::Int, $($arg: $arg::Int),*| -> Checked<Dynamic> {
                            let args = ArgCtx::of_call(&call);
                            let handle = H::take_int(handle, &args)?;
                            $(let $arg = $arg::take_int($arg, &args)?;)*
                            int(handle, $($arg),*).map(ScriptValue::engine)
                        },
                    );
                }
            }
        }

        impl<F, R, H, $($arg),*> MethodFn<WithCtx, H, ($($arg,)*), R> for F
        where
            F: Fn(&Ctx, H, $($arg),*) -> Checked<R> + 'static,
            R: ScriptType,
            H: ScriptArg,
            $($arg: ScriptArg,)*
        {
            const ARITY: usize = {
                let args: &[&str] = &[$(stringify!($arg)),*];
                args.len()
            };

            fn args() -> ArgKinds {
                args_of(&[$($arg::NAMES),*], &[$($arg::ENUM),*])
            }

            fn output() -> TypeId {
                TypeId::of::<R>()
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
                    move |call: NativeCallContext<'_>, handle: H::Raw, $($arg: $arg::Raw),*| -> Checked<Dynamic> {
                        let args = ArgCtx::of_call(&call);
                        let handle = H::take(handle, &args)?;
                        $(let $arg = $arg::take($arg, &args)?;)*
                        each(args.get(), handle, $($arg),*).map(ScriptValue::engine)
                    },
                );
                if [H::NUMBER, $($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(
                        name,
                        move |call: NativeCallContext<'_>, handle: H::Int, $($arg: $arg::Int),*| -> Checked<Dynamic> {
                            let args = ArgCtx::of_call(&call);
                            let handle = H::take_int(handle, &args)?;
                            $(let $arg = $arg::take_int($arg, &args)?;)*
                            int(args.get(), handle, $($arg),*).map(ScriptValue::engine)
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
