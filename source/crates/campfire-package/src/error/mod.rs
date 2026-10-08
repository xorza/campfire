use std::io;
use std::path::PathBuf;

use campfire_capabilities::PackagePath;
use campfire_common::Fingerprint;
use derive_more::Display;
use thiserror::Error;
use toml::de::Error as TomlError;

use crate::error::load_problem::LoadProblem;

pub(crate) mod box_problem;
pub(crate) mod build_problem;
pub(crate) mod choice_problem;
pub(crate) mod ctx_misuse;
pub(crate) mod delivery_problem;
pub(crate) mod effect_problem;
pub(crate) mod gather_problem;
pub(crate) mod item_problem;
pub(crate) mod limit;
pub(crate) mod load_problem;
pub(crate) mod locale_problem;
pub(crate) mod place;
pub(crate) mod script_problem;

/// Why a package file does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Error)]
pub enum ContentError {
    /// The file does not read.
    #[error("{path} does not read")]
    Io {
        path: PackagePath,
        #[source]
        error: io::Error,
    },
    /// The data file is not TOML of the expected shape.
    #[error("{path} does not parse as its data")]
    Data {
        path: PackagePath,
        #[source]
        error: TomlError,
    },
    /// A directory of packages, or of a package's files, does not read.
    #[error("{} does not list", .dir.display())]
    Scan {
        dir: PathBuf,
        #[source]
        error: io::Error,
    },
    /// An entry of a package is neither a file nor a directory, such as a link.
    #[error("{}: neither a file nor a directory", .0.display())]
    NotAFile(PathBuf),
    /// A path in a package is not UTF-8, so no file list can name it.
    #[error("{}: path is not UTF-8", .0.display())]
    NotUtf8(PathBuf),
    /// A file's name in a package is no package path, as one holding `\` is not.
    #[error("{}: no path a package can name", .0.display())]
    NotPath(PathBuf),
    /// The package holds more files than a load takes.
    #[error("more files than a package holds")]
    TooManyFiles,
    /// A file a load reads is larger than one it reads.
    #[error("{0} is larger than a file a load reads")]
    TooLarge(PackagePath),
    /// The files a load reads are larger together than it reads of a package.
    #[error("more to read than a package holds")]
    TooMuchToRead,
}

/// Why a store does not give the packages that session terms name.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The store holds no mode of the fingerprint the terms name.
    #[error("no mode of the session's fingerprint is held")]
    UnknownMode,
    /// The terms name another count of dependencies than the mode's manifest.
    #[error("the session names another count of dependencies than the mode")]
    DependencyCount,
    /// The store holds no package of the fingerprint the terms give the dependency of this name.
    #[error("no package of the fingerprint the session gives {0:?} is held")]
    MissingDependency(String),
    /// The packages do not load.
    #[error(transparent)]
    Load(LoadError),
}

/// Why a mode's packages do not load: `problem`, in the package `package`.
#[derive(Debug, Error)]
#[error("package {package}")]
pub struct LoadError {
    pub package: PackageRef,
    #[source]
    pub problem: Box<LoadProblem>,
}

/// Which package a load error is in: by its name, once its manifest named it; else where it was
/// read from, a directory, or the fingerprint a session named it by.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub enum PackageRef {
    #[display("{_0}")]
    Name(String),
    #[display("at {}", _0.display())]
    Dir(PathBuf),
    #[display("of fingerprint {_0}")]
    Fingerprint(Fingerprint),
}

impl LoadError {
    pub fn new(package: PackageRef, problem: LoadProblem) -> LoadError {
        LoadError {
            package,
            problem: Box::new(problem),
        }
    }

    /// `problem`, of the package its manifest names `name`.
    pub(crate) fn of(name: &str, problem: LoadProblem) -> LoadError {
        LoadError::new(PackageRef::Name(name.to_owned()), problem)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use campfire_capabilities::{DeclaredName, ModifierProblem};

    use super::*;
    use crate::error::locale_problem::LocaleProblem;
    use crate::error::script_problem::ScriptProblem;

    /// The message of `error` and of each error under it, down to the root.
    fn chain(error: &(dyn Error + 'static)) -> Vec<String> {
        let mut messages = vec![error.to_string()];
        let mut source = error.source();
        while let Some(cause) = source {
            messages.push(cause.to_string());
            source = cause.source();
        }
        messages
    }

    #[test]
    fn a_load_error_names_its_package_and_its_problem_and_the_error_under_it() {
        let error = |problem| LoadError::of("hero", problem);
        let path = |text| PackagePath::parse(text).unwrap();
        // A problem that holds an error adds its own step, and one that only holds one is the
        // error itself, so no step shows twice.
        let cases = [
            (
                LoadProblem::Modifier {
                    modifier: DeclaredName::new("haste").unwrap(),
                    problem: ModifierProblem::Time,
                },
                &[
                    "package hero",
                    "modifier \"haste\"",
                    "a time negative or too large to count in ticks",
                ][..],
            ),
            (
                LoadProblem::Content(ContentError::NotUtf8(PathBuf::from("x"))),
                &["package hero", "x: path is not UTF-8"],
            ),
            (
                LoadProblem::Script {
                    path: path("scripts/a.rhai"),
                    problem: ScriptProblem::Missing,
                },
                &["package hero", "scripts/a.rhai", "named, but not held"],
            ),
            (
                LoadProblem::Locale {
                    path: path("en.ftl"),
                    problem: LocaleProblem::FileName,
                },
                &[
                    "package hero",
                    "en.ftl",
                    "not <language>.ftl, its language in its canonical spelling",
                ],
            ),
            (
                LoadProblem::WrongKind,
                &["package hero", "not a package of the kind its place needs"],
            ),
        ];
        for (problem, messages) in cases {
            assert_eq!(chain(&error(problem)), messages);
        }
    }
}
