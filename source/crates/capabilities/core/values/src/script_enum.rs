use campfire_common::NamedEnum;

use crate::engine_enum::EngineEnum;

/// The Rust type of an engine enum, whose values scripts hold as its members, each named in data
/// as `NamedEnum` names it.
pub trait ScriptEnum: NamedEnum {
    const ENUM: EngineEnum;
    /// Each member's name in scripts, in the order of `ALL`: `enum_all!`'s `IDENTS`.
    const SCRIPT_NAMES: &'static [&'static str];

    /// Each member by its name in scripts, in order.
    fn members() -> impl Iterator<Item = (&'static str, Self)> {
        const {
            assert!(
                Self::SCRIPT_NAMES.len() == Self::ALL.len(),
                "a script name for each member"
            );
        };
        Self::SCRIPT_NAMES
            .iter()
            .copied()
            .zip(Self::ALL.iter().copied())
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::fmt::Debug;

    use serde::de::{DeserializeOwned, IntoDeserializer, value};

    use crate::script_enum::ScriptEnum;

    /// Checks that each member of `T` reads from data by the name `named` reads it by, and that
    /// a text that names no member names none.
    pub fn named_as_data<T: ScriptEnum + DeserializeOwned + Debug>() {
        for (_, member) in T::members() {
            let text = IntoDeserializer::<value::Error>::into_deserializer(member.name());
            assert_eq!(T::deserialize(text).unwrap(), member);
            assert_eq!(T::named(member.name()), Some(member));
        }
        assert_eq!(T::named("none"), None);
    }
}
