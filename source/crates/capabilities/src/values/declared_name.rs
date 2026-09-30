use std::fmt;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A name a mode declares in its data and scripts use, such as a damage kind, a stat or a
/// resource: a lowercase letter, then lowercase letters, digits and underscores.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclaredName(Box<str>);

impl DeclaredName {
    /// `text` as a name; `None` unless it is of that form.
    pub fn new(text: &str) -> Option<DeclaredName> {
        let mut chars = text.chars();
        let first = chars.next()?;
        let rest = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
        (first.is_ascii_lowercase() && chars.all(rest)).then(|| DeclaredName(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeclaredName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DeclaredName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<DeclaredName, D::Error> {
        let text = String::deserialize(deserializer)?;
        DeclaredName::new(&text).ok_or_else(|| {
            D::Error::custom(format!(
                "{text:?} is not a name: a lowercase letter, then lowercase letters, digits and \
                 underscores"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_starts_with_a_letter_and_holds_letters_digits_and_underscores() {
        for good in ["magic", "armor_pen_pct", "x2"] {
            assert_eq!(
                DeclaredName::new(good).map(|name| name.to_string()),
                Some(good.to_owned())
            );
        }
        for bad in ["", "2x", "_x", "Magic", "fire damage", "armor-pen", "é"] {
            assert_eq!(DeclaredName::new(bad), None, "{bad:?}");
        }
    }
}
