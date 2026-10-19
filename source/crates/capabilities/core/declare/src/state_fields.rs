use std::collections::BTreeSet;

use campfire_common::DeclaredName;

use crate::fact::Fact;

/// Every state field the packages' data declares, of a modifier or a unit type: what a script
/// may name after `.state`, as its handle has no type the load knows. A fact the modules whose
/// data declares state add to.
#[derive(Debug, Default)]
pub struct StateFields(BTreeSet<DeclaredName>);

impl Fact for StateFields {}

impl StateFields {
    /// Adds the fields `fields`.
    pub fn extend<'f>(&mut self, fields: impl IntoIterator<Item = &'f DeclaredName>) {
        self.0.extend(fields.into_iter().cloned());
    }

    /// Whether data declares the field `name`.
    pub fn has(&self, name: &str) -> bool {
        self.0.contains(name)
    }
}
