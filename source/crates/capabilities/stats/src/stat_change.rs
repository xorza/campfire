use campfire_values::Number;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::stat_op::StatOp;

/// A modifier's change of one stat a stack, as data writes it: a number adds it; `{ pct }` and
/// `{ cut }` name their operation, and `{ add }` may name the plain one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatChange {
    pub op: StatOp,
    pub value: Number,
}

/// A table with no operation, or with two, fails to read.
impl<'de> Deserialize<'de> for StatChange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Table {
            add: Option<Number>,
            pct: Option<Number>,
            cut: Option<Number>,
        }
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Form {
            Table(Table),
            Plain(Number),
        }
        match Form::deserialize(deserializer)? {
            Form::Plain(value) => Ok(Self {
                op: StatOp::Add,
                value,
            }),
            Form::Table(table) => match (table.add, table.pct, table.cut) {
                (Some(value), None, None) => Ok(Self {
                    op: StatOp::Add,
                    value,
                }),
                (None, Some(value), None) => Ok(Self {
                    op: StatOp::Pct,
                    value,
                }),
                (None, None, Some(value)) => Ok(Self {
                    op: StatOp::Cut,
                    value,
                }),
                _ => Err(D::Error::custom(
                    "a stat change is a number, or one of add, pct and cut",
                )),
            },
        }
    }
}
