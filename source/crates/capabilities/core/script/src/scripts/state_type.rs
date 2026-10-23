use serde::{Deserialize, Serialize};

/// The type of a state field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateType {
    Int,
    Num,
    Bool,
    String,
    Unit,
    UnitList,
    Vec,
    IntList,
    NumList,
    BoolList,
    StringList,
    /// A text that is one of the field's declared members.
    Enum,
}

impl StateType {
    /// The types a timer's data may take, in the order a value tries them: every type but an
    /// enum, whose members its field declares, and a timer's data has no field.
    pub fn timer_order() -> impl Iterator<Item = Self> {
        core::iter::successors(Some(Self::Int), |kind| kind.next_for_timer())
    }

    /// The type a timer's data tries after this one. Each type has its arm, so a new type takes
    /// its place in the order as it is declared.
    const fn next_for_timer(self) -> Option<Self> {
        match self {
            Self::Int => Some(Self::Num),
            Self::Num => Some(Self::Bool),
            Self::Bool => Some(Self::String),
            Self::String => Some(Self::Unit),
            Self::Unit => Some(Self::UnitList),
            Self::UnitList => Some(Self::Vec),
            Self::Vec => Some(Self::IntList),
            Self::IntList => Some(Self::NumList),
            Self::NumList => Some(Self::BoolList),
            Self::BoolList => Some(Self::StringList),
            Self::StringList | Self::Enum => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timer_tries_each_type_but_an_enum_once() {
        let order: Vec<StateType> = StateType::timer_order().collect();
        assert_eq!(
            order,
            [
                StateType::Int,
                StateType::Num,
                StateType::Bool,
                StateType::String,
                StateType::Unit,
                StateType::UnitList,
                StateType::Vec,
                StateType::IntList,
                StateType::NumList,
                StateType::BoolList,
                StateType::StringList,
            ]
        );
    }
}
