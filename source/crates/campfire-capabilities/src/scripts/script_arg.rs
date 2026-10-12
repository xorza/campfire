use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString, Map, Variant};
use campfire_sim::Position;

use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
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

/// Implements `ScriptArg` for `$ty`, a name of `$kind` that `$find` resolves, given the call's
/// `ctx` and the name a script gives; a name it does not resolve fails the call.
macro_rules! name_script_arg {
    ($ty:ty, $kind:expr, $find:expr) => {
        impl $crate::scripts::script_arg::ScriptArg for $ty {
            type Raw = campfire_script::rhai::ImmutableString;
            type Int = campfire_script::rhai::ImmutableString;
            const NAMES: Option<$crate::scripts::name_kind::NameKind> = Some($kind);
            const ENUM: Option<$crate::values::engine_enum::EngineEnum> = None;
            const NUMBER: bool = false;

            fn take(
                raw: campfire_script::rhai::ImmutableString,
                ctx: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<$ty> {
                let find: fn(&$crate::scripts::ctx::Ctx, &str) -> Option<$ty> = $find;
                find(ctx.get(), raw.as_str()).ok_or_else(|| {
                    $crate::scripts::error::ApiError::Unknown($kind)
                        .fail()
                        .into()
                })
            }

            fn take_int(
                raw: campfire_script::rhai::ImmutableString,
                ctx: &$crate::scripts::arg_ctx::ArgCtx<'_>,
            ) -> $crate::scripts::error::Checked<$ty> {
                <$ty as $crate::scripts::script_arg::ScriptArg>::take(raw, ctx)
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

plain_script_arg!(
    (),
    INT,
    bool,
    ImmutableString,
    Array,
    Map,
    Dynamic,
    Position,
    Vec3
);
enum_script_arg!(Relation);

impl ScriptArg for Num {
    type Raw = Num;
    type Int = INT;
    const NAMES: Option<NameKind> = None;
    const ENUM: Option<EngineEnum> = None;
    const NUMBER: bool = true;

    fn take(raw: Num, _: &ArgCtx<'_>) -> Checked<Num> {
        Ok(raw)
    }

    fn take_int(raw: INT, _: &ArgCtx<'_>) -> Checked<Num> {
        ApiError::num(raw)
    }
}

name_script_arg!(DamageKind, NameKind::DamageKind, |ctx, name| {
    ctx.view().names().damage_kinds.named(name)
});
