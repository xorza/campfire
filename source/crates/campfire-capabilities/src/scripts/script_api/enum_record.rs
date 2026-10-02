use crate::values::engine_enum::EngineEnum;

/// An engine enum as the registry binds it: its members, by their names in scripts, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumRecord {
    pub engine_enum: EngineEnum,
    pub members: Vec<&'static str>,
}

impl EnumRecord {
    /// The functions of every enum's module, beside its members.
    pub const FUNCTIONS: [&'static str; 1] = ["named"];
    /// The functions on every enum's members: `==`, `!=` and `to_string`.
    pub const MEMBER_FUNCTIONS: [&'static str; 3] = ["==", "!=", "to_string"];

    /// Whether `name` is one of its members.
    pub fn has(&self, name: &str) -> bool {
        self.members.contains(&name)
    }
}
