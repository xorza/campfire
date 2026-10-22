use std::any::TypeId;
use std::rc::Rc;

use campfire_values::EngineEnum;
use rhai::{Dynamic, Engine, NativeCallContext};

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::Checked;
use crate::scripts::given_names::GivenNames;
use crate::scripts::member_decl::{ArgKinds, MemberDecl};
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_arg::ScriptArg;
use crate::scripts::script_output::ScriptOutput;
use crate::scripts::script_value::ScriptType;
use campfire_values::NameKind;

/// A form of a call or a value of `ctx`, which takes `ctx` first and then its arguments, each a
/// `ScriptArg`. Rhai calls it with each argument as the script gives it, and a second time, its
/// integer form, with an integer for each `Num`; a call in a role it does not serve fails before
/// it runs.
pub trait CtxFn<Args, R> {
    /// How many arguments it takes, `ctx` aside.
    const ARITY: usize;

    /// The name of each argument's type, by place, as the reference shows it.
    const TYPES: &'static [&'static str];

    /// What each argument names, and the engine enum each takes, by place.
    fn args() -> ArgKinds;

    /// The names the value it gives is; none for a value that names nothing.
    const NAMES: Option<GivenNames>;

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

    /// The name of each argument's type, by place, as the reference shows it.
    const TYPES: &'static [&'static str];

    /// What each argument names, and the engine enum each takes, by place.
    fn args() -> ArgKinds;

    /// The names the value it gives is; none for a value that names nothing.
    const NAMES: Option<GivenNames>;

    /// The type of the value it gives.
    fn output() -> TypeId;

    fn register(self, engine: &mut Engine, name: &str);
}

/// A form of a value of `ctx` or a field's getter of a handle `H`, which takes the handle, and the
/// call's `ctx` before it when its marker `M` is `WithCtx`. It gives its value `O`: a value, or
/// one that fails the call when its marker `K` is `Fallible`.
pub trait GetFn<M, H, O, K> {
    /// The names the value it gives is; none for a value that names nothing.
    const NAMES: Option<GivenNames>;

    /// The type of the value it gives.
    fn output() -> TypeId;

    fn register(self, engine: &mut Engine, name: &str);
}

/// What a form gives: its value, or with the marker `Fallible`, its value or the error that
/// fails the call.
pub(crate) trait ScriptReturn<K> {
    /// The value.
    type Output: ScriptOutput;

    /// The value in the engine's form, or the error.
    fn checked(self) -> Checked<Dynamic>;
}

/// The marker of a form that gives its value.
#[derive(Debug)]
pub struct Plain;

/// The marker of a form that may fail the call.
#[derive(Debug)]
pub struct Fallible;

impl<R: ScriptOutput> ScriptReturn<Plain> for R {
    type Output = R;

    fn checked(self) -> Checked<Dynamic> {
        Ok(self.into_engine())
    }
}

impl<R: ScriptOutput> ScriptReturn<Fallible> for Checked<R> {
    type Output = R;

    fn checked(self) -> Checked<Dynamic> {
        self.map(R::into_engine)
    }
}

impl<F, H, O, K> GetFn<WithoutCtx, H, O, K> for F
where
    F: Fn(&mut H) -> O + 'static,
    H: ScriptType,
    O: ScriptReturn<K>,
{
    const NAMES: Option<GivenNames> = <O::Output as ScriptOutput>::NAMES;

    fn output() -> TypeId {
        TypeId::of::<<O::Output as ScriptOutput>::Value>()
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
    const NAMES: Option<GivenNames> = <O::Output as ScriptOutput>::NAMES;

    fn output() -> TypeId {
        TypeId::of::<<O::Output as ScriptOutput>::Value>()
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

/// How many of the arguments whose `NUMBER` is `flags` are `Num`s.
const fn numbers(flags: &[bool]) -> usize {
    let mut count = 0;
    let mut at = 0;
    while at < flags.len() {
        if flags[at] {
            count += 1;
        }
        at += 1;
    }
    count
}

/// What arguments that name `names` and take `enums` name and take, by place, among the
/// arguments a spec records.
fn args_of(names: &[Option<NameKind>], enums: &[Option<EngineEnum>]) -> ArgKinds {
    assert!(
        names.len() <= MemberDecl::ARGS,
        "a form takes at most the arguments a spec records"
    );
    let mut args = ArgKinds {
        names: [None; MemberDecl::ARGS],
        enums: [None; MemberDecl::ARGS],
    };
    for (at, (&name, &taken)) in names.iter().zip(enums).enumerate() {
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
            R: ScriptOutput,
            $($arg: ScriptArg,)*
        {
            const ARITY: usize = {
                let args: &[&str] = &[$(stringify!($arg)),*];
                args.len()
            };

            const TYPES: &'static [&'static str] = &[$($arg::TYPE),*];

            fn args() -> ArgKinds {
                args_of(&[$($arg::NAME_KIND),*], &[$($arg::ENUM),*])
            }

            const NAMES: Option<GivenNames> = R::NAMES;

            fn output() -> TypeId {
                TypeId::of::<R::Value>()
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
                    ctx.admit(roles)?;
                    let args = ArgCtx::Ctx(ctx);
                    $(let $arg = $arg::take($arg, &args)?;)*
                    each(ctx, $($arg),*).map(R::into_engine)
                });
                const {
                    assert!(
                        numbers(&[$($arg::NUMBER),*]) <= 1,
                        "a member's integer form takes each `Num` as an integer, so it takes one `Num` at most"
                    );
                }
                if [$($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(name, move |ctx: &mut Ctx, $($arg: $arg::Int),*| -> Checked<Dynamic> {
                        ctx.admit(roles)?;
                        let args = ArgCtx::Ctx(ctx);
                        $(let $arg = $arg::take_int($arg, &args)?;)*
                        int(ctx, $($arg),*).map(R::into_engine)
                    });
                }
            }
        }

        impl<F, R, H, $($arg),*> MethodFn<WithoutCtx, H, ($($arg,)*), R> for F
        where
            F: Fn(H, $($arg),*) -> Checked<R> + 'static,
            R: ScriptOutput,
            H: ScriptArg,
            $($arg: ScriptArg,)*
        {
            const ARITY: usize = {
                let args: &[&str] = &[$(stringify!($arg)),*];
                args.len()
            };

            const TYPES: &'static [&'static str] = &[$($arg::TYPE),*];

            fn args() -> ArgKinds {
                args_of(&[$($arg::NAME_KIND),*], &[$($arg::ENUM),*])
            }

            const NAMES: Option<GivenNames> = R::NAMES;

            fn output() -> TypeId {
                TypeId::of::<R::Value>()
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
                        each(handle, $($arg),*).map(R::into_engine)
                    },
                );
                const {
                    assert!(
                        numbers(&[H::NUMBER, $($arg::NUMBER),*]) <= 1,
                        "a method's integer form takes each `Num` as an integer, so it takes one `Num` at most"
                    );
                }
                if [H::NUMBER, $($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(
                        name,
                        move |call: NativeCallContext<'_>, handle: H::Int, $($arg: $arg::Int),*| -> Checked<Dynamic> {
                            let args = ArgCtx::of_call(&call);
                            let handle = H::take_int(handle, &args)?;
                            $(let $arg = $arg::take_int($arg, &args)?;)*
                            int(handle, $($arg),*).map(R::into_engine)
                        },
                    );
                }
            }
        }

        impl<F, R, H, $($arg),*> MethodFn<WithCtx, H, ($($arg,)*), R> for F
        where
            F: Fn(&Ctx, H, $($arg),*) -> Checked<R> + 'static,
            R: ScriptOutput,
            H: ScriptArg,
            $($arg: ScriptArg,)*
        {
            const ARITY: usize = {
                let args: &[&str] = &[$(stringify!($arg)),*];
                args.len()
            };

            const TYPES: &'static [&'static str] = &[$($arg::TYPE),*];

            fn args() -> ArgKinds {
                args_of(&[$($arg::NAME_KIND),*], &[$($arg::ENUM),*])
            }

            const NAMES: Option<GivenNames> = R::NAMES;

            fn output() -> TypeId {
                TypeId::of::<R::Value>()
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
                        each(args.get(), handle, $($arg),*).map(R::into_engine)
                    },
                );
                const {
                    assert!(
                        numbers(&[H::NUMBER, $($arg::NUMBER),*]) <= 1,
                        "a method's integer form takes each `Num` as an integer, so it takes one `Num` at most"
                    );
                }
                if [H::NUMBER, $($arg::NUMBER),*].contains(&true) {
                    engine.register_fn(
                        name,
                        move |call: NativeCallContext<'_>, handle: H::Int, $($arg: $arg::Int),*| -> Checked<Dynamic> {
                            let args = ArgCtx::of_call(&call);
                            let handle = H::take_int(handle, &args)?;
                            $(let $arg = $arg::take_int($arg, &args)?;)*
                            int(args.get(), handle, $($arg),*).map(R::into_engine)
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
