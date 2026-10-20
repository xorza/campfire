use campfire_values::{PackageIndex, TextId};

use crate::error::BookPlace;
use crate::fact::Fact;

/// The texts the packages' data names, each at its place, which the package's own language
/// gives: a fact the modules whose data names a text add to, which the load checks against
/// each package's locale files (02-engine-core.md, A module checks the rules of its own data).
#[derive(Debug, Default)]
pub struct NamedTexts(Vec<NamedText>);

/// The text `text`, which data of the package at `package` names at `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedText {
    pub package: PackageIndex,
    pub at: BookPlace,
    pub text: TextId,
}

impl Fact for NamedTexts {}

impl NamedTexts {
    /// Adds that data of the package at `package` names `text` at `at`.
    pub fn name(&mut self, package: PackageIndex, at: BookPlace, text: &TextId) {
        self.0.push(NamedText {
            package,
            at,
            text: text.clone(),
        });
    }

    /// Each text data names, in the order the modules named them.
    pub fn iter(&self) -> std::slice::Iter<'_, NamedText> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a NamedTexts {
    type Item = &'a NamedText;
    type IntoIter = std::slice::Iter<'a, NamedText>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
