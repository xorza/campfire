use std::fmt;

use campfire_common::Json;
use serde::Serialize;

/// A field an event logs as its JSON text, with `%`, for a value with no text form of its own,
/// such as an order's action; `LogLine::json` reads it back.
#[derive(Debug, Clone, Copy)]
pub struct JsonText<'a, T>(pub &'a T);

impl<T: Serialize> fmt::Display for JsonText<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = Json::write(self.0).expect("a logged value writes as JSON");
        f.write_str(&text)
    }
}
