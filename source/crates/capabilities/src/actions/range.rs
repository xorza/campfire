use std::str::FromStr;

use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// How far an action reaches. In data: meters as a decimal string, or `global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Meters(Num),
    Global,
}
impl<'de> Deserialize<'de> for Range {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Range, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "global" {
            return Ok(Range::Global);
        }
        Num::from_str(&text)
            .ok()
            .filter(|meters| *meters >= Num::ZERO)
            .map(Range::Meters)
            .ok_or_else(|| Error::custom(format!("range {text:?} is not meters or global")))
    }
}
