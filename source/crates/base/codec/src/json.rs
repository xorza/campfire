use std::{fmt, io, str};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::JsonError;

/// JSON, the format of the values an event logs as text, the log's lines read back, and a glTF
/// model's JSON.
#[derive(Debug)]
pub struct Json;

impl Json {
    /// The value `text` holds.
    pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, JsonError> {
        Ok(serde_json::from_str(text)?)
    }

    /// `value` as JSON text, with no space between its tokens. A type JSON cannot hold, as a map
    /// keyed by other than text, is a caller's error, and panics.
    pub fn write<T: Serialize + ?Sized>(value: &T) -> String {
        serde_json::to_string(value).expect("a caller writes only a type JSON holds")
    }

    /// Writes `value` as `write` gives it into `out`, with no text between: an error only for
    /// `out`'s own.
    pub fn write_into<T: Serialize + ?Sized>(value: &T, out: &mut dyn fmt::Write) -> fmt::Result {
        let mut adapter = FmtWriter { out, failed: false };
        let written = serde_json::to_writer(&mut adapter, value);
        if adapter.failed {
            return Err(fmt::Error);
        }
        written.expect("a caller writes only a type JSON holds");
        Ok(())
    }
}

/// A text sink as the bytes JSON writes go into: each piece JSON writes is whole UTF-8, as it
/// cuts a string only at an escape, which is ASCII.
struct FmtWriter<'a> {
    out: &'a mut dyn fmt::Write,
    /// Whether `out` refused a piece.
    failed: bool,
}

impl io::Write for FmtWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let text = str::from_utf8(bytes).expect("JSON writes whole UTF-8 pieces");
        self.out.write_str(text).map_err(|fmt::Error| {
            self.failed = true;
            io::Error::other("the text sink refused a piece")
        })?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl fmt::Debug for FmtWriter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FmtWriter")
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::panic;

    use super::*;

    #[test]
    fn a_value_writes_compact_and_parses_back_and_bad_text_fails() {
        let value = BTreeMap::from([("a", vec![1, 2])]);
        assert_eq!(Json::write(&value), r#"{"a":[1,2]}"#);
        assert_eq!(
            Json::parse::<BTreeMap<String, Vec<u8>>>(r#"{"a":[1,2]}"#).unwrap()["a"],
            [1, 2]
        );
        assert!(Json::parse::<Vec<u8>>("[1,").is_err());
        assert!(panic::catch_unwind(|| Json::write(&BTreeMap::from([((1, 2), 3)]))).is_err());
        // Into a text sink, the same text, a string's escapes and its letters past ASCII whole.
        let text = BTreeMap::from([("ä \"q\"", "é\n")]);
        let mut out = String::new();
        Json::write_into(&text, &mut out).unwrap();
        assert_eq!(out, Json::write(&text));
    }
}
