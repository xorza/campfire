use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_values::{EngineEnum, Relation};
use rhai::{Array, Dynamic, ImmutableString, Variant};

use crate::script_host::error::{NumError, Raised};
use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::view::filter::Filter;
use crate::scripts::view::unit::Unit;

/// An argument of a call or a method of the script API: what Rhai passes, and what the member
/// takes of it. A name resolves to its id, its kind of name recorded in the member's spec, and a
/// `Num` takes an integer too, in the member's integer form. Its impl lives with its type, or
/// here for a type of a layer below the scripts', of another crate, or of `view`, which an
/// argument's `ArgCtx` reads, so `view` imports no `ScriptArg`.
pub trait ScriptArg: Sized + 'static {
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
#[macro_export]
macro_rules! plain_script_arg {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::ScriptArg for $ty {
            type Raw = $ty;
            type Int = $ty;
            const NAMES: Option<$crate::NameKind> = None;
            const ENUM: Option<::campfire_values::EngineEnum> = None;
            const NUMBER: bool = false;

            fn take(
                raw: $ty,
                _: &$crate::ArgCtx<'_>,
            ) -> $crate::Checked<$ty> {
                Ok(raw)
            }

            fn take_int(
                raw: $ty,
                _: &$crate::ArgCtx<'_>,
            ) -> $crate::Checked<$ty> {
                Ok(raw)
            }
        }
    )*};
}

/// An argument a script gives as a text, which a call reads as what it names.
pub trait TextArg: Sized + 'static {
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
#[macro_export]
macro_rules! name_script_arg {
    ($ty:ty, $kind:expr, $find:expr) => {
        impl $crate::TextArg for $ty {
            const NAMES: $crate::NameKind = $kind;

            fn take_text(
                text: $crate::ScriptText,
                ctx: &$crate::ArgCtx<'_>,
            ) -> $crate::Checked<$ty> {
                let find: fn(&$crate::Ctx, &str) -> Option<$ty> = $find;
                find(ctx.get(), &text).ok_or_else(|| $crate::ApiError::Unknown($kind).fail().into())
            }
        }
    };
}

/// Implements `ScriptArg` for each engine enum, which Rhai passes as itself.
#[macro_export]
macro_rules! enum_script_arg {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::ScriptArg for $ty {
            type Raw = $ty;
            type Int = $ty;
            const NAMES: Option<$crate::NameKind> = None;
            const ENUM: Option<::campfire_values::EngineEnum> =
                Some(<$ty as ::campfire_values::ScriptEnum>::ENUM);
            const NUMBER: bool = false;

            fn take(
                raw: $ty,
                _: &$crate::ArgCtx<'_>,
            ) -> $crate::Checked<$ty> {
                Ok(raw)
            }

            fn take_int(
                raw: $ty,
                _: &$crate::ArgCtx<'_>,
            ) -> $crate::Checked<$ty> {
                Ok(raw)
            }
        }
    )*};
}

plain_script_arg!((), i64, bool, Vec3, Unit);
enum_script_arg!(Relation);

impl ScriptArg for Num {
    type Raw = Self;
    type Int = i64;
    const NAMES: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = true;

    fn take(raw: Self, _: &ArgCtx<'_>) -> Checked<Self> {
        Ok(raw)
    }

    /// A script's integer as a `Num`; one beyond a `Num` fails the call, as the number API's
    /// own does.
    fn take_int(raw: i64, _: &ArgCtx<'_>) -> Checked<Self> {
        Self::from_int(raw).ok_or_else(|| Raised::error(NumError::IntegerBeyondNum).into())
    }
}

/// A player a script names by its slot, which the session has; another fails the call.
impl ScriptArg for PlayerSlot {
    type Raw = i64;
    type Int = i64;
    const NAMES: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;

    fn take(raw: i64, ctx: &ArgCtx<'_>) -> Checked<Self> {
        ctx.get().view().names().player(raw)
    }

    fn take_int(raw: i64, ctx: &ArgCtx<'_>) -> Checked<Self> {
        Self::take(raw, ctx)
    }
}

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

/// A filter a script gives, its tags among the match's.
impl TextArg for Filter {
    const NAMES: NameKind = NameKind::Filter;

    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Self> {
        let filter = ctx.get().view().filter(&text);
        Ok(filter.map_err(ApiError::fail)?)
    }
}
