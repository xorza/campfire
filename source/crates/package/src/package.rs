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

    /// The place of the script at `path` in `scripts`.
    pub fn script_index(&self, path: &PackagePath) -> Option<usize> {
        self.scripts.iter().position(|script| script.path == *path)
    }
}
