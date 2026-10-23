use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use campfire_package_format::PackageFiles;
use campfire_values::PackagePath;
use derive_more::Display;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
    /// The directory of a package's text files, each `<language>.ftl`.
    pub(crate) const DIR: &str = PackageFiles::LOCALE;

    /// `text` as a language; `None` unless it is an identifier in its canonical spelling.
    pub fn parse(text: &str) -> Option<Self> {
        let identifier: LanguageIdentifier = text.parse().ok()?;
        (identifier == text).then(|| Self {
            text: text.into(),
            identifier,
        })
    }

    /// The path of its text file in a package: `locale/<language>.ftl`.
    pub fn text_path(&self) -> PackagePath {
        PackagePath::parse(&format!("{}/{}.ftl", Self::DIR, self.text))
            .expect("a language's spelling is a package path's name")
    }

    /// The language whose text file `path` is, as `text_path` spells it; none for any other path.
    pub(crate) fn of_text_path(path: &PackagePath) -> Option<Self> {
        let name = path.as_str().strip_prefix(Self::DIR)?.strip_prefix('/')?;
        Self::parse(name.strip_suffix(".ftl")?)
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
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for Language {}

impl PartialOrd for Language {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Language {
    fn cmp(&self, other: &Self) -> Ordering {
        self.text.cmp(&other.text)
    }
}

impl Hash for Language {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

/// As its spelling, which `Deserialize` reads.
impl Serialize for Language {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| {
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
        // Its text file is `locale/<language>.ftl`, and only such a path names a language.
        assert_eq!(brazil.text_path().as_str(), "locale/pt-BR.ftl");
        assert_eq!(Language::of_text_path(&brazil.text_path()), Some(brazil));
        for other in [
            "locale/pt-br.ftl",
            "locale/en.toml",
            "localen.ftl",
            "data/en.ftl",
        ] {
            let path = PackagePath::parse(other).unwrap();
            assert_eq!(Language::of_text_path(&path), None, "{other}");
        }
    }
}
