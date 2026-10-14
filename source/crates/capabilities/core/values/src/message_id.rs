use std::borrow::Borrow;

use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// The id of a message of a package's human text, as Fluent names one: a letter, then letters,
/// digits, `-` and `_`. Data names a message by it, and the package's `locale/` files give its
/// text.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(Box<str>);

impl MessageId {
    /// `text` as an id; `None` unless it is of that form.
    pub fn new(text: &str) -> Option<MessageId> {
        let mut chars = text.chars();
        let first = chars.next()?;
        let rest = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        (first.is_ascii_alphabetic() && chars.all(rest)).then(|| MessageId(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// By its text, so a map keyed by ids finds the id a file gives.
impl Borrow<str> for MessageId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for MessageId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<MessageId, D::Error> {
        let text = String::deserialize(deserializer)?;
        MessageId::new(&text).ok_or_else(|| {
            D::Error::custom(format!(
                "{text:?} is not a message id: a letter, then letters, digits, - and _"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_starts_with_a_letter_and_holds_letters_digits_dashes_and_underscores() {
        for good in ["hero-name", "Name", "a", "tooltip_2"] {
            assert_eq!(
                MessageId::new(good).map(|id| id.to_string()),
                Some(good.to_owned())
            );
        }
        for bad in ["", "-name", "2name", "_name", "hero name", "hero.name", "é"] {
            assert_eq!(MessageId::new(bad), None, "{bad:?}");
        }
    }
}
