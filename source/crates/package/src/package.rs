use std::collections::BTreeMap;

use campfire_capabilities::{ActionData, ApiVersion, DeliveryData};
use campfire_content::{Fingerprint, PackagePath};
use campfire_script::ScriptHost;

use crate::error::{LoadError, LoadProblem, PackageRef};
use crate::files::units_data::UnitTypeFile;
use crate::package_files::PackageFiles;
use crate::script_facts::ScriptFacts;

/// Where a package holds its game scripts.
const SCRIPTS: &str = "scripts";

/// A package as the load read it: its name, fingerprint and target release, and each of its
/// scripts.
#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub fingerprint: Fingerprint,
    pub(crate) api: ApiVersion,
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
    /// The package of `files`, named `name` and targeting `api`, with every script it holds.
    pub(crate) fn read(
        files: &PackageFiles,
        name: String,
        api: ApiVersion,
        parser: &ScriptHost,
    ) -> Result<Package, LoadError> {
        let fail = |problem| LoadError {
            package: PackageRef::Name(name.clone()),
            problem: Box::new(problem),
        };
        let mut scripts = Vec::new();
        for path in files.files_under(SCRIPTS) {
            let source = files
                .read_text(path)
                .map_err(|error| fail(LoadProblem::Content(error)))?;
            let ast = parser.parse(source).map_err(|error| {
                fail(LoadProblem::Script {
                    path: path.clone(),
                    error,
                })
            })?;
            scripts.push(Script {
                facts: ScriptFacts::read(&ast),
                path: path.clone(),
                source: source.to_owned(),
            });
        }
        Ok(Package {
            name,
            fingerprint: files.fingerprint(),
            api,
            scripts,
        })
    }

    /// The script at `path`.
    pub(crate) fn script(&self, path: &PackagePath) -> Option<&Script> {
        Some(&self.scripts[self.script_index(path)?])
    }

    /// The abilities of `abilities`, this package's, that apply each modifier: those whose data
    /// names it, those whose area type of `units` holds it inside, and those whose script adds
    /// it.
    pub(crate) fn appliers<'a>(
        &'a self,
        abilities: &'a BTreeMap<String, ActionData>,
        units: &'a BTreeMap<String, UnitTypeFile>,
    ) -> BTreeMap<&'a str, Vec<&'a ActionData>> {
        let mut appliers: BTreeMap<&str, Vec<&ActionData>> = BTreeMap::new();
        for ability in abilities.values() {
            let inside = match &ability.delivery {
                Some(DeliveryData::Area { unit_type }) => units
                    .get(unit_type)
                    .and_then(|unit_type| unit_type.area.as_ref()),
                _ => None,
            }
            .into_iter()
            .flat_map(|area| area.inside.modifiers());
            let scripted = ability
                .script
                .as_ref()
                .and_then(|path| self.script(path))
                .into_iter()
                .flat_map(|script| script.facts.modifiers.iter().map(String::as_str));
            for id in ability.modifiers().chain(inside).chain(scripted) {
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
