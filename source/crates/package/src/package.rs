use std::collections::BTreeMap;

use campfire_capabilities::{
    ActionData, DeclaredName, DeliveryData, NameKind, ScriptApi, UnitTypeFile,
};
use campfire_content::{Fingerprint, PackagePath};
use campfire_script::ScriptHost;

use crate::error::{LoadError, LoadProblem, ScriptProblem};
use crate::files::manifest::PackageHeader;
use crate::package_files::PackageFiles;
use crate::package_text::PackageText;
use crate::script_facts::ScriptFacts;

/// Where a package holds its game scripts.
const SCRIPTS: &str = "scripts";

/// A package as the load read it: its header, its fingerprint, each of its scripts, and its
/// human text.
#[derive(Debug)]
pub struct Package {
    pub header: PackageHeader,
    pub fingerprint: Fingerprint,
    /// Every file under `scripts/`, by path.
    pub scripts: Vec<Script>,
    pub text: PackageText,
}

#[derive(Debug)]
pub struct Script {
    pub path: PackagePath,
    pub source: String,
    pub(crate) facts: ScriptFacts,
}

impl Package {
    /// The package of `files`, of `header`, with every script it holds and its text.
    pub(crate) fn read(
        files: &PackageFiles,
        header: &PackageHeader,
        parser: &ScriptHost,
        api: &ScriptApi,
    ) -> Result<Package, LoadError> {
        let name = header.name.clone();
        let fail = |problem| LoadError::of(&name, problem);
        let mut scripts = Vec::new();
        for path in files.files_under(SCRIPTS) {
            let source = files
                .read_text(path)
                .map_err(|error| fail(LoadProblem::Content(error)))?;
            let ast = parser.parse(source).map_err(|error| {
                fail(LoadProblem::Script {
                    path: path.clone(),
                    problem: ScriptProblem::Compile(error),
                })
            })?;
            scripts.push(Script {
                facts: ScriptFacts::read(&ast, api),
                path: path.clone(),
                source: source.to_owned(),
            });
        }
        let text = PackageText::read(files, &header.language).map_err(fail)?;
        Ok(Package {
            header: header.clone(),
            fingerprint: files.fingerprint(),
            scripts,
            text,
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
        abilities: &'a BTreeMap<DeclaredName, ActionData>,
        units: &'a BTreeMap<DeclaredName, UnitTypeFile>,
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
                .flat_map(|script| script.facts.names_of(NameKind::Modifier));
            let named = ability.modifiers().chain(inside).map(DeclaredName::as_str);
            for id in named.chain(scripted) {
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
