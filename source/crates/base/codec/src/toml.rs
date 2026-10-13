use crate::error::TomlError;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// TOML, the format of a package's data files and the configuration a person writes.
#[derive(Debug)]
pub struct Toml;

impl Toml {
    /// The value `text` holds.
    pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, TomlError> {
        Ok(toml::from_str(text)?)
    }

    /// `value` as a TOML document. A type TOML cannot hold, as one of no table at its top, is a
    /// caller's error, and panics.
    pub fn write<T: Serialize + ?Sized>(value: &T) -> String {
        toml::to_string(value).expect("a caller writes only a type TOML holds")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::panic;

    use super::*;

    #[test]
    fn a_table_writes_and_parses_back_and_bad_text_fails() {
        let table = BTreeMap::from([("cell", 10), ("step", 5)]);
        let text = Toml::write(&table);
        assert_eq!(text, "cell = 10\nstep = 5\n");
        assert_eq!(
            Toml::parse::<BTreeMap<String, i64>>(&text).unwrap()["step"],
            5
        );
        assert!(Toml::parse::<BTreeMap<String, i64>>("cell = ").is_err());
        assert!(panic::catch_unwind(|| Toml::write(&5)).is_err());
    }
}
