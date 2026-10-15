use serde::Deserialize;

/// The type of a state field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateType {
    Int,
    Num,
    Bool,
    String,
    Entity,
    EntityList,
    Pos,
    Vec,
}
