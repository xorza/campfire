use serde::Serialize;
use serde::de::DeserializeOwned;

/// JSON, the format of the values an event logs as text, the log's lines read back, and a glTF
/// model's JSON.
#[derive(Debug)]
pub struct Json;

impl Json {
    /// The value `text` holds.
    pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// `value` as JSON text, with no space between its tokens; an error for a value JSON cannot
    /// hold, as a map keyed by other than text.
    pub fn write<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
        serde_json::to_string(value)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn a_value_writes_compact_and_parses_back_and_bad_text_fails() {
        let value = BTreeMap::from([("a", vec![1, 2])]);
        assert_eq!(Json::write(&value).unwrap(), r#"{"a":[1,2]}"#);
        assert_eq!(
            Json::parse::<BTreeMap<String, Vec<u8>>>(r#"{"a":[1,2]}"#).unwrap()["a"],
            [1, 2]
        );
        assert!(Json::parse::<Vec<u8>>("[1,").is_err());
        assert!(Json::write(&BTreeMap::from([((1, 2), 3)])).is_err());
    }
}
