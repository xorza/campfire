use std::path::PathBuf;

use campfire_common::PackageFingerprint;
use derive_more::Display;
use thiserror::Error;

use crate::error::load_problem::LoadProblem;
use crate::package_name::PackageName;

pub(crate) mod load_problem;
pub(crate) mod locale_problem;
pub(crate) mod place;
pub(crate) mod script_problem;

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
    Name(PackageName),
    #[display("at {}", _0.display())]
    Dir(PathBuf),
    #[display("of fingerprint {_0}")]
    PackageFingerprint(PackageFingerprint),
}

/// Why a store does not give the packages that session terms name.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("no mode of the session's fingerprint is held")]
    UnknownMode,
    #[error("the session names another count of dependencies than the mode")]
    DependencyCount,
    #[error("no package of the fingerprint the session gives {0:?} is held")]
    MissingDependency(PackageName),
    #[error(transparent)]
    Load(LoadError),
}

impl LoadError {
    pub fn new(package: PackageRef, problem: LoadProblem) -> Self {
        Self {
            package,
            problem: Box::new(problem),
        }
    }

    /// `problem`, of the package its manifest names `name`.
    pub(crate) fn of(name: &PackageName, problem: LoadProblem) -> Self {
        Self::new(PackageRef::Name(name.clone()), problem)
    }
}

#[cfg(test)]
mod tests;
