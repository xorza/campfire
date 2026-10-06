use std::error::Error;
use std::path::PathBuf;
use std::{fmt, io};

use campfire_capabilities::PackagePath;
use campfire_common::Fingerprint;
use thiserror::Error;
use toml::de::Error as TomlError;

use crate::error::load_problem::LoadProblem;
use crate::error::script_problem::ScriptProblem;

pub(crate) mod choice_problem;
pub(crate) mod ctx_misuse;
pub(crate) mod delivery_problem;
pub(crate) mod effect_problem;
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
    #[error("{path}: {error}")]
    Io {
        path: PackagePath,
        #[source]
        error: io::Error,
    },
    /// The data file is not TOML of the expected shape.
    #[error("{path}: {error}")]
    Data {
        path: PackagePath,
        #[source]
        error: TomlError,
    },
    /// A directory of packages, or of a package's files, does not read.
    #[error("{}: {error}", .dir.display())]
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
    #[error("{0}")]
    Load(#[source] LoadError),
}

/// Why a mode's packages do not load: `problem`, in the package `package`.
#[derive(Debug)]
pub struct LoadError {
    pub package: PackageRef,
    pub problem: Box<LoadProblem>,
}

/// Which package a load error is in: by its name, once its manifest named it; else where it was
/// read from, a directory, or the fingerprint a session named it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageRef {
    Name(String),
    Dir(PathBuf),
    Fingerprint(Fingerprint),
}

impl fmt::Display for PackageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageRef::Name(name) => f.write_str(name),
            PackageRef::Dir(dir) => write!(f, "at {}", dir.display()),
            PackageRef::Fingerprint(fingerprint) => write!(f, "of fingerprint {fingerprint}"),
        }
    }
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

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "package {}: {}", self.package, self.problem)
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &*self.problem {
            LoadProblem::Content(error) => Some(error),
            LoadProblem::Script {
                problem: ScriptProblem::Compile(error),
                ..
            } => Some(error),
            LoadProblem::Mode(error) => Some(error),
            _ => None,
        }
    }
}
