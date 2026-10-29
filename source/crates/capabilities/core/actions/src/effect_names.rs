use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_names::{NameTables, UnknownName};
use campfire_values::{NameId, NameKey, Named, PackageIndex, Param};

/// The match's ids of the names an action's effect lists give, by the match's name tables, and
/// the action's params by their places: what an action's lists resolve their names by, in the
/// load or in a match's world; an error for a name the match does not have.
#[derive(Debug, Clone, Copy)]
pub struct EffectNames<'a> {
    tables: &'a NameTables,
    /// The action's params, by name.
    params: &'a BTreeMap<DeclaredName, Param>,
    package: PackageIndex,
}

impl<'a> EffectNames<'a> {
    /// The names of the lists of an action of `package` with `params`, by `tables`.
    pub const fn new(
        tables: &'a NameTables,
        params: &'a BTreeMap<DeclaredName, Param>,
        package: PackageIndex,
    ) -> Self {
        Self {
            tables,
            params,
            package,
        }
    }

    /// The package of the action whose lists resolve, whose own names they name.
    pub const fn package(&self) -> PackageIndex {
        self.package
    }

    /// The place among the action's params of the param `name`.
    pub fn param(&self, name: &DeclaredName) -> Result<usize, UnknownName> {
        // A param's place is its place among the action's, by name, as the params' table holds
        // them.
        let param = self.params.keys().position(|held| held == name);
        param.ok_or_else(|| UnknownName::new(Param::NAMES, name))
    }

    /// The id of type `I` named `key`.
    pub fn named<I: NameId + Named + 'static>(
        &self,
        key: impl NameKey<I::Scope>,
    ) -> Result<I, UnknownName> {
        let id = self.tables.named::<I>(key);
        id.ok_or_else(|| UnknownName::new(I::NAMES, key.text()))
    }
}
