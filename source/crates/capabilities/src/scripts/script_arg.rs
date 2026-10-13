use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Array, Dynamic, ImmutableString, Variant};
use campfire_sim::Position;

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_list::ScriptList;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::values::damage_kind::DamageKind;
use crate::values::engine_enum::EngineEnum;
use crate::values::relation::Relation;

/// An argument of a call or a method of the script API: what Rhai passes, and what the member
/// takes of it. A name resolves to its id, its kind of name recorded in the member's spec, and a
/// `Num` takes an integer too, in the member's integer form. Its impl lives with its type, or
/// here for a type of a layer below the scripts' or of another crate.
pub(crate) trait ScriptArg: Sized + 'static {
    /// What Rhai passes.
    type Raw: Variant + Clone;
    /// What Rhai passes in the member's integer form: an integer for a `Num`, else `Raw`.
    type Int: Variant + Clone;
    /// The kind of name it is; none for one that names nothing.
    const NAMES: Option<NameKind>;
    /// The engine enum it is a member of; none for another value.
    const ENUM: Option<EngineEnum>;
    /// Whether it is a `Num`, which the member's integer form takes as an integer.
    const NUMBER: bool;

    /// The argument `raw` gives in the call `ctx` runs; an error fails the call.
    fn take(raw: Self::Raw, ctx: &ArgCtx<'_>) -> Checked<Self>;

    /// The argument `raw` gives in the member's integer form.
    fn take_int(raw: Self::Int, ctx: &ArgCtx<'_>) -> Checked<Self>;
}

/// Implements `ScriptArg` for each type, which Rhai passes as itself and which names nothing.
macro_rules! plain_script_arg {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::scripts::script_arg::ScriptArg for $ty {
            type Raw = $ty;
            type Int = $ty;
            const NAMES: Option<$crate::scripts::name_kind::NameKind> = None;
            const ENUM: Option<$crate::values::engine_enum::EngineEnum> = None;
            const NUMBER: bool = false;

            fn take(
                raw: $ty,
                _: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<$ty> {
                Ok(raw)
            }

            fn take_int(
                raw: $ty,
                _: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<$ty> {
                Ok(raw)
            }
        }
    )*};
}

/// An argument a script gives as a text, which a call reads as what it names.
pub(crate) trait TextArg: Sized + 'static {
    /// The kind of name it is.
    const NAMES: NameKind;

    /// The argument `text` names in the call `ctx` runs; an error fails the call.
    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Self>;
}

impl<T: TextArg> ScriptArg for T {
    type Raw = ImmutableString;
    type Int = ImmutableString;
    const NAMES: Option<NameKind> = Some(<T as TextArg>::NAMES);
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;

    fn take(raw: ImmutableString, ctx: &ArgCtx<'_>) -> Checked<T> {
        T::take_text(ScriptText::of_engine(raw), ctx)
    }

    fn take_int(raw: ImmutableString, ctx: &ArgCtx<'_>) -> Checked<T> {
        T::take_text(ScriptText::of_engine(raw), ctx)
    }
}

/// Implements `TextArg` for `$ty`, a name of `$kind` that `$find` resolves, given the call's
/// `ctx` and the name a script gives; a name it does not resolve fails the call.
macro_rules! name_script_arg {
    ($ty:ty, $kind:expr, $find:expr) => {
        impl $crate::scripts::script_arg::TextArg for $ty {
            const NAMES: $crate::scripts::name_kind::NameKind = $kind;

            fn take_text(
                text: $crate::scripts::script_text::ScriptText,
                ctx: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<$ty> {
                let find: fn(&$crate::scripts::ctx::Ctx, &str) -> Option<$ty> = $find;
                find(ctx.get(), &text).ok_or_else(|| {
                    $crate::scripts::error::ApiError::Unknown($kind)
                        .fail()
                        .into()
                })
            }
        }
    };
}

/// Implements `ScriptArg` for each engine enum, which Rhai passes as itself.
macro_rules! enum_script_arg {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::scripts::script_arg::ScriptArg for $ty {
            type Raw = $ty;
            type Int = $ty;
            const NAMES: Option<$crate::scripts::name_kind::NameKind> = None;
            const ENUM: Option<$crate::values::engine_enum::EngineEnum> =
                Some(<$ty as $crate::values::script_enum::ScriptEnum>::ENUM);
            const NUMBER: bool = false;

            fn take(
                raw: $ty,
                _: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<$ty> {
                Ok(raw)
            }

            fn take_int(
                raw: $ty,
                _: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<$ty> {
                Ok(raw)
            }
        }
    )*};
}

pub(crate) use enum_script_arg;
pub(crate) use name_script_arg;
pub(crate) use plain_script_arg;

plain_script_arg!((), i64, bool, Position, Vec3);
enum_script_arg!(Relation);

impl ScriptArg for Num {
    type Raw = Num;
    type Int = i64;
    const NAMES: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = true;

    fn take(raw: Num, _: &ArgCtx<'_>) -> Checked<Num> {
        Ok(raw)
    }

    fn take_int(raw: i64, _: &ArgCtx<'_>) -> Checked<Num> {
        ApiError::num(raw)
    }
}

name_script_arg!(DamageKind, NameKind::DamageKind, |ctx, name| {
    ctx.view().names().damage_kinds.named(name)
});

/// Implements `ScriptArg` for each type of `scripts` that stands for an engine type, which Rhai
/// passes as the engine's, and `$of` takes.
macro_rules! engine_script_arg {
    ($($ty:ty: $raw:ty => $of:expr),* $(,)?) => {$(
        impl ScriptArg for $ty {
            type Raw = $raw;
            type Int = $raw;
            const NAMES: Option<NameKind> = None;
            const ENUM: Option<EngineEnum> = None;
            const NUMBER: bool = false;

            fn take(raw: $raw, _: &ArgCtx<'_>) -> Checked<$ty> {
                Ok($of(raw))
            }

            fn take_int(raw: $raw, _: &ArgCtx<'_>) -> Checked<$ty> {
                Ok($of(raw))
            }
        }
    )*};
}

engine_script_arg!(
    ScriptValue: Dynamic => ScriptValue::of_engine,
    ScriptText: ImmutableString => ScriptText::of_engine,
    ScriptList: Array => ScriptList::of_engine,
);
