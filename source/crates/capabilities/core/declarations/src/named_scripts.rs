use campfire_common::DeclaredName;
use campfire_script::ScriptRole;
use campfire_values::{PackageIndex, PackagePath};

use crate::fact::Fact;

/// The scripts the packages' data names, each with a role it serves and the params it may read
/// there: a fact the modules whose data names a script add to, which the load's checks of the
/// scripts read (02-engine-core.md, A module checks the rules of its own data).
#[derive(Debug, Default)]
pub struct NamedScripts(Vec<NamedScript>);

/// A script that data of the package at `package` names, to serve `role`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedScript {
    pub package: PackageIndex,
    pub path: PackagePath,
    pub role: ScriptRole,
    pub params: ScriptParams,
}

/// The params a script may read in a role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptParams {
    /// Those its data gives it, as an action's.
    Named(Vec<DeclaredName>),
    /// The mode's, as an AI reads them.
    Mode,
}

impl Fact for NamedScripts {}

impl NamedScripts {
    /// Adds that data of the package at `package` names the script at `path` to serve `role`,
    /// reading `params`.
    pub fn name(
        &mut self,
        package: PackageIndex,
        path: &PackagePath,
        role: ScriptRole,
        params: ScriptParams,
    ) {
        self.0.push(NamedScript {
            package,
            path: path.clone(),
            role,
            params,
        });
    }

    /// Each script data names, in the order the modules named them.
    pub fn iter(&self) -> std::slice::Iter<'_, NamedScript> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a NamedScripts {
    type Item = &'a NamedScript;
    type IntoIter = std::slice::Iter<'a, NamedScript>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
