use campfire_common::PackageFingerprint;
use campfire_script::{Hook, ScriptApi, ScriptFacts, ScriptFunction, ScriptHost};
use campfire_values::PackagePath;

use crate::error::LoadError;
use crate::error::load_problem::LoadProblem;
use crate::error::script_problem::ScriptProblem;
use crate::files::package_header::PackageHeader;
use crate::package_files::{PackageFiles, SCRIPTS};
use crate::package_reader::PackageReader;
use crate::package_text::PackageText;

/// A package as the load read it: its header, each of its scripts, its human text, and its files,
/// whose index gives its fingerprint.
#[derive(Debug)]
pub struct Package {
    pub header: PackageHeader,
    /// Every file under `scripts/`, by path.
    pub scripts: Vec<Script>,
    pub text: PackageText,
    /// Each of its files, read on demand, as its client reads its assets.
    pub files: PackageReader,
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
        let named = |function: &ScriptFunction| Hook::named(&function.name) == Some(hook);
        self.facts.functions.iter().any(named)
    }
}

impl Package {
    /// The package of `files`, of `header`, with every script it holds and its text.
    pub(crate) fn read(
        files: &PackageFiles,
        header: &PackageHeader,
        host: &ScriptHost,
        api: &ScriptApi,
    ) -> Result<Self, LoadError> {
        let name = header.name.clone();
        let fail = |problem| LoadError::of(&name, problem);
        let mut scripts = Vec::new();
        for path in files.files_under(SCRIPTS) {
            let source = files
                .read_text(path)
                .map_err(|error| fail(LoadProblem::Content(error)))?;
            let facts = ScriptFacts::read(host, source, api).map_err(|error| {
                fail(LoadProblem::Script {
                    path: path.clone(),
                    problem: ScriptProblem::Compile(error),
                })
            })?;
            scripts.push(Script {
                facts,
                path: path.clone(),
                source: source.to_owned(),
            });
        }
        let text = PackageText::read(files, &header.language).map_err(fail)?;
        Ok(Self {
            header: header.clone(),
            scripts,
            text,
            files: files.reader().clone(),
        })
    }

    /// The fingerprint of its files' index.
    pub const fn fingerprint(&self) -> PackageFingerprint {
        self.files.fingerprint()
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
