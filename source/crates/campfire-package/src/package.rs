use campfire_capabilities::{Hook, PackagePath, ScriptApi};
use campfire_common::Fingerprint;
use campfire_script::ScriptHost;

use crate::error::LoadError;
use crate::error::load_problem::LoadProblem;
use crate::error::script_problem::ScriptProblem;
use crate::files::package_header::PackageHeader;
use crate::package_files::PackageFiles;
use crate::package_text::PackageText;
use crate::script_facts::{Function, ScriptFacts};

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

impl Script {
    /// Whether it defines `hook`.
    pub(crate) fn defines(&self, hook: Hook) -> bool {
        let named = |function: &Function| Hook::named(&function.name) == Some(hook);
        self.facts.functions.iter().any(named)
    }
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

    /// The place of the script at `path` in `scripts`.
    pub fn script_index(&self, path: &PackagePath) -> Option<usize> {
        self.scripts.iter().position(|script| script.path == *path)
    }
}
