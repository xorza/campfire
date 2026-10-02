use campfire_script::rhai::Variant;

use crate::values::engine_enum::EngineEnum;

/// The Rust type of an engine enum, whose values scripts hold as its members.
pub(crate) trait ScriptEnum: Variant + Clone + Copy + PartialEq {
    const ENUM: EngineEnum;
    /// Each member, by its name in scripts, in order.
    const MEMBERS: &'static [(&'static str, Self)];

    /// The member as data names it, which `to_string` gives and `named` reads.
    fn data_name(self) -> &'static str;

    /// The member data names `text`.
    fn named(text: &str) -> Option<Self> {
        Self::MEMBERS
            .iter()
            .map(|&(_, member)| member)
            .find(|member| member.data_name() == text)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::fmt::Debug;

    use serde::de::DeserializeOwned;

    use crate::values::script_enum::ScriptEnum;

    /// Checks that each member of `T` reads from data by the name `named` reads it by, and that
    /// a text that names no member names none.
    pub(crate) fn named_as_data<T: ScriptEnum + DeserializeOwned + Debug>() {
        for &(_, member) in T::MEMBERS {
            let text = toml::Value::String(member.data_name().to_owned());
            assert_eq!(text.try_into::<T>().unwrap(), member);
            assert_eq!(T::named(member.data_name()), Some(member));
        }
        assert_eq!(T::named("none"), None);
    }
}
