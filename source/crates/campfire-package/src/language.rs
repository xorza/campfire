use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use unic_langid::LanguageIdentifier;

/// A language, as a Unicode language identifier in its canonical spelling, such as `en` or
/// `pt-BR`: the language of a package's own text, and the name of each of its `locale/` files,
/// so each language has one spelling.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Language(String);

impl Language {
    /// `text` as a language; `None` unless it is an identifier in its canonical spelling.
    pub fn parse(text: &str) -> Option<Language> {
        let id: LanguageIdentifier = text.parse().ok()?;
        let canonical = id.to_string();
        (canonical == text).then_some(Language(canonical))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn identifier(&self) -> LanguageIdentifier {
        self.0.parse().expect("a language is an identifier")
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
                Language::parse(good).map(|language| language.0),
                Some(good.to_owned())
            );
        }
        for bad in ["", "EN", "pt-br", "pt_BR", "en-", "e", "123"] {
            assert_eq!(Language::parse(bad), None, "{bad:?}");
        }
        assert_eq!(
            Language::parse("pt-BR").unwrap().identifier().to_string(),
            "pt-BR"
        );
    }
}
