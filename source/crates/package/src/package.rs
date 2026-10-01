use std::collections::BTreeMap;

use campfire_capabilities::AbilityData;
use campfire_content::{Fingerprint, PackagePath};
use campfire_script::ScriptHost;

use crate::error::{LoadError, LoadProblem};
use crate::files::version::Version;
use crate::package_dir::PackageDir;
use crate::script_facts::ScriptFacts;

/// Where a package holds its game scripts.
const SCRIPTS: &str = "scripts";

/// A package as the load read it: its name, fingerprint and target release, and each of its
/// scripts.
#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub fingerprint: Fingerprint,
    pub(crate) engine: Version,
    /// Every file under `scripts/`, by path.
    pub scripts: Vec<Script>,
}

#[derive(Debug)]
pub struct Script {
    pub path: PackagePath,
    pub source: String,
    pub(crate) facts: ScriptFacts,
}

impl Package {
    /// The package in `dir`, named `name` and targeting `engine`, with every script it holds.
    pub(crate) fn read(
        dir: &PackageDir,
        name: String,
        engine: Version,
        parser: &ScriptHost,
    ) -> Result<Package, LoadError> {
        let fail = |problem| LoadError {
            package: name.clone(),
            problem: Box::new(problem),
        };
        let fingerprint = dir
            .fingerprint()
            .map_err(|error| fail(LoadProblem::Content(error)))?;
        let mut scripts = Vec::new();
        let paths = dir
            .files_under(SCRIPTS)
            .map_err(|error| fail(LoadProblem::Content(error)))?;
        for path in paths {
            let source = dir
                .read_text(&path)
                .map_err(|error| fail(LoadProblem::Content(error)))?;
            let ast = parser.parse(&source).map_err(|error| {
                fail(LoadProblem::Script {
                    path: path.clone(),
                    error,
                })
            })?;
            scripts.push(Script {
                facts: ScriptFacts::read(&ast),
                path,
                source,
            });
        }
        Ok(Package {
            name,
            fingerprint,
            engine,
            scripts,
        })
    }

    /// The script at `path`.
    pub(crate) fn script(&self, path: &PackagePath) -> Option<&Script> {
        Some(&self.scripts[self.script_index(path)?])
    }

    /// The abilities of `abilities`, this package's, that apply each modifier: those whose data
    /// names it, and those whose script adds it.
    pub(crate) fn appliers<'a>(
        &'a self,
        abilities: &'a BTreeMap<String, AbilityData>,
    ) -> BTreeMap<&'a str, Vec<&'a AbilityData>> {
        let mut appliers: BTreeMap<&str, Vec<&AbilityData>> = BTreeMap::new();
        for ability in abilities.values() {
            let scripted = ability
                .script
                .as_ref()
                .and_then(|path| self.script(path))
                .into_iter()
                .flat_map(|script| script.facts.modifiers.iter().map(String::as_str));
            for id in ability.modifiers().chain(scripted) {
                appliers.entry(id).or_default().push(ability);
            }
        }
        appliers
    }

    /// The place of the script at `path` in `scripts`.
    pub fn script_index(&self, path: &PackagePath) -> Option<usize> {
        self.scripts.iter().position(|script| script.path == *path)
    }
}
