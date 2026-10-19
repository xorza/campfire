use campfire_math::Vec3;

use crate::scripts::script_value::ScriptType;

/// What a script holds a name on: a type the API registers, as its handle's members are, or
/// `ctx`, by the type's name in scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApiHolder {
    name: &'static str,
    title: &'static str,
}

/// A type the API registers for scripts to hold, a type of the module that registers it: its
/// name in scripts, and the title the reference gives its members
/// (06-script-api.md, A holder is its registered type).
pub trait ScriptHandle: ScriptType {
    const NAME: &'static str;
    const TITLE: &'static str;
}

impl ApiHolder {
    /// The holder of the calls and values of `ctx`.
    pub const CTX: Self = Self {
        name: "Ctx",
        title: "`ctx`",
    };

    /// The holder of the members of `T`'s handles.
    pub const fn of<T: ScriptHandle>() -> Self {
        Self {
            name: T::NAME,
            title: T::TITLE,
        }
    }

    /// Its type's name in scripts.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The holder as the reference titles it.
    pub const fn title(self) -> &'static str {
        self.title
    }
}

impl ScriptHandle for Vec3 {
    const NAME: &'static str = "Vector";
    const TITLE: &'static str = "Vector";
}
