use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_names::Filter;
use campfire_values::{EngineEnum, Relation};
use campfire_values::{NameKind, Named};
use rhai::{Array, Dynamic, ImmutableString, Map, Variant};

use crate::script_host::error::{NumError, Raised};
use crate::scripts::api_holder::ScriptHandle;
use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::player_handle::PlayerHandle;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::script_value::script_map::ScriptMap;
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
    const NAME_KIND: Option<NameKind>;
    /// The engine enum it is a member of; none for another value.
    const ENUM: Option<EngineEnum>;
    /// Whether it is a `Num`, which the member's integer form takes as an integer.
    const NUMBER: bool;
    /// The name of its type, as the reference shows it.
    const TYPE: &'static str;

    /// The argument `raw` gives in the call `ctx` runs; an error fails the call.
    fn take(raw: Self::Raw, ctx: &ArgCtx<'_>) -> Checked<Self>;

    /// The argument `raw` gives in the member's integer form.
    fn take_int(raw: Self::Int, ctx: &ArgCtx<'_>) -> Checked<Self>;
}

/// Implements `ScriptArg` for each type, which Rhai passes as itself and which names nothing,
/// its type named as the type of a handle is, or as given.
#[macro_export]
macro_rules! plain_script_arg {
    (@arg $ty:ty, $name:expr) => {
        impl $crate::ScriptArg for $ty {
            type Raw = $ty;
            type Int = $ty;
            const NAME_KIND: Option<::campfire_values::NameKind> = None;
            const ENUM: Option<::campfire_values::EngineEnum> = None;
            const NUMBER: bool = false;
            const TYPE: &'static str = $name;

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
    };
    ($($ty:ty),* $(,)?) => {$(
        $crate::plain_script_arg!(@arg $ty, <$ty as $crate::ScriptHandle>::NAME);
    )*};
    ($($ty:ty: $name:literal),* $(,)?) => {$(
        $crate::plain_script_arg!(@arg $ty, $name);
    )*};
}

/// Implements `ScriptArg` for each type, a time a script gives in milliseconds, which Rhai
/// passes as an integer: the type's `of` at the match's rate, whose refusal fails the call.
macro_rules! ms_script_arg {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::scripts::script_arg::ScriptArg for $ty {
            type Raw = i64;
            type Int = i64;
            const NAME_KIND: Option<::campfire_values::NameKind> = None;
            const ENUM: Option<::campfire_values::EngineEnum> = None;
            const NUMBER: bool = false;
            const TYPE: &'static str = "ms";

            fn take(
                ms: i64,
                ctx: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<Self> {
                Ok(Self::of(ctx.get().view().rate(), ms)
                    .map_err($crate::scripts::error::ApiError::fail)?)
            }

            fn take_int(
                ms: i64,
                ctx: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<Self> {
                Self::take(ms, ctx)
            }
        }
    )*};
}

pub(crate) use ms_script_arg;

/// An argument a script gives as a text, which a call reads as what it names.
pub trait TextArg: Named + Sized + 'static {
    /// The argument `text` names in the call `ctx` runs; an error fails the call.
    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Self>;
}

impl<T: TextArg> ScriptArg for T {
    type Raw = ImmutableString;
    type Int = ImmutableString;
    const NAME_KIND: Option<NameKind> = Some(<T as Named>::NAMES);
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;
    const TYPE: &'static str = "text";

    fn take(raw: ImmutableString, ctx: &ArgCtx<'_>) -> Checked<T> {
        T::take_text(ScriptText::of_engine(raw), ctx)
    }

    fn take_int(raw: ImmutableString, ctx: &ArgCtx<'_>) -> Checked<T> {
        T::take_text(ScriptText::of_engine(raw), ctx)
    }
}

/// Implements `TextArg` for `$ty`, a name of its kind that `$find` resolves, given the call's
/// `ctx` and the name a script gives; a name it does not resolve fails the call.
#[macro_export]
macro_rules! name_script_arg {
    ($ty:ty, $find:expr) => {
        impl $crate::TextArg for $ty {
            fn take_text(
                text: $crate::ScriptText,
                ctx: &$crate::ArgCtx<'_>,
            ) -> $crate::Checked<$ty> {
                let find: fn(&$crate::Ctx, &str) -> Option<$ty> = $find;
                let kind = <$ty as ::campfire_values::Named>::NAMES;
                find(ctx.get(), &text).ok_or_else(|| $crate::ApiError::Unknown(kind).fail().into())
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
            const NAME_KIND: Option<::campfire_values::NameKind> = None;
            const ENUM: Option<::campfire_values::EngineEnum> =
                Some(<$ty as ::campfire_values::ScriptEnum>::ENUM);
            const NUMBER: bool = false;
            const TYPE: &'static str = stringify!($ty);

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

plain_script_arg!((): "()", i64: "int", bool: "bool");
plain_script_arg!(Vec3, Unit, PlayerHandle);
enum_script_arg!(Relation);

impl ScriptArg for Num {
    type Raw = Self;
    type Int = i64;
    const NAME_KIND: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = true;
    const TYPE: &'static str = "Num";

    fn take(raw: Self, _: &ArgCtx<'_>) -> Checked<Self> {
        Ok(raw)
    }

    /// A script's integer as a `Num`; one beyond a `Num` fails the call, as the number API's
    /// own does.
    fn take_int(raw: i64, _: &ArgCtx<'_>) -> Checked<Self> {
        Self::from_int(raw).ok_or_else(|| Raised::error(NumError::IntegerBeyondNum).into())
    }
}

/// A player a script holds, as the slot of its handle.
impl ScriptArg for PlayerSlot {
    type Raw = PlayerHandle;
    type Int = PlayerHandle;
    const NAME_KIND: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = false;
    const TYPE: &'static str = PlayerHandle::NAME;

    fn take(raw: PlayerHandle, _: &ArgCtx<'_>) -> Checked<Self> {
        Ok(raw.player_slot())
    }

    fn take_int(raw: PlayerHandle, _: &ArgCtx<'_>) -> Checked<Self> {
        Ok(raw.player_slot())
    }
}

/// Implements `ScriptArg` for each type of `scripts` that stands for an engine type, which Rhai
/// passes as the engine's, and `$of` takes.
macro_rules! engine_script_arg {
    ($($ty:ty: $raw:ty => $of:expr, $name:literal),* $(,)?) => {$(
        impl ScriptArg for $ty {
            type Raw = $raw;
            type Int = $raw;
            const NAME_KIND: Option<NameKind> = None;
            const ENUM: Option<EngineEnum> = None;
            const NUMBER: bool = false;
            const TYPE: &'static str = $name;

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
    ScriptValue: Dynamic => ScriptValue::of_engine, "any",
    ScriptText: ImmutableString => ScriptText::of_engine, "text",
    ScriptList: Array => ScriptList::of_engine, "array",
    ScriptMap: Map => ScriptMap::of_engine, "map",
);

/// A filter a script gives, its tags among the match's.
impl TextArg for Filter {
    fn take_text(text: ScriptText, ctx: &ArgCtx<'_>) -> Checked<Self> {
        let filter = ctx.get().view().filter(&text);
        Ok(filter.map_err(ApiError::fail)?)
    }
}
