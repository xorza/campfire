use campfire_values::{MessageId, PackageIndex};

use crate::error::BookPlace;
use crate::fact::Fact;

/// The messages the packages' data names, each at its place, which the package's own language
/// gives: a fact the modules whose data names a message add to, which the load checks against
/// each package's texts (02-engine-core.md, A module checks the rules of its own data).
#[derive(Debug, Default)]
pub struct NamedMessages(Vec<NamedMessage>);

/// The message `message`, which data of the package at `package` names at `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedMessage {
    pub package: PackageIndex,
    pub at: BookPlace,
    pub message: MessageId,
}

impl Fact for NamedMessages {}

impl NamedMessages {
    /// Adds that data of the package at `package` names `message` at `at`.
    pub fn name(&mut self, package: PackageIndex, at: BookPlace, message: &MessageId) {
        self.0.push(NamedMessage {
            package,
            at,
            message: message.clone(),
        });
    }

    /// Each message data names, in the order the modules named them.
    pub fn iter(&self) -> std::slice::Iter<'_, NamedMessage> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a NamedMessages {
    type Item = &'a NamedMessage;
    type IntoIter = std::slice::Iter<'a, NamedMessage>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
