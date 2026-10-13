use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use unic_langid::LanguageIdentifier;

/// A language, as a Unicode language identifier in its canonical spelling, such as `en` or
/// `pt-BR`: the language of a package's own text, and the name of each of its `locale/` files,
/// so each language has one spelling. It holds the identifier parsed once, and its spelling,
/// which orders and compares it.
#[derive(Debug, Display, Clone)]
#[display("{text}")]
pub struct Language {
    text: Box<str>,
    identifier: LanguageIdentifier,
}

impl Language {
    /// `text` as a language; `None` unless it is an identifier in its canonical spelling.
    pub fn parse(text: &str) -> Option<Language> {
        let identifier: LanguageIdentifier = text.parse().ok()?;
        (identifier == text).then(|| Language {
            text: text.into(),
            identifier,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub const fn identifier(&self) -> &LanguageIdentifier {
        &self.identifier
    }
}

/// One spelling for each identifier, so the spelling alone decides.
impl PartialEq for Language {
    fn eq(&self, other: &Language) -> bool {
        self.text == other.text
    }
}

impl Eq for Language {}

impl PartialOrd for Language {
    fn partial_cmp(&self, other: &Language) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Language {
    fn cmp(&self, other: &Language) -> Ordering {
        self.text.cmp(&other.text)
    }
}

impl Hash for Language {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Language, D::Error> {
        let text = String::deserialize(deserializer)?;
        Language::parse(&text).ok_or_else(|| {
            D::Error::custom(format!(
                "{text:?} is not a language identifier in its canonical spelling"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_is_an_identifier_in_its_one_spelling() {
        for good in ["en", "de", "pt-BR", "zh-Hant-TW", "sr-Latn"] {
            assert_eq!(
                Language::parse(good).as_ref().map(Language::as_str),
                Some(good)
            );
        }
        for bad in ["", "EN", "pt-br", "pt_BR", "en-", "e", "123"] {
            assert_eq!(Language::parse(bad), None, "{bad:?}");
        }
        let brazil = Language::parse("pt-BR").unwrap();
        assert_eq!(brazil.identifier().to_string(), "pt-BR");
        assert_eq!(brazil.to_string(), "pt-BR");
        // Ordered by spelling, as the locale files are named.
        assert!(Language::parse("de").unwrap() < brazil);
    }
}
