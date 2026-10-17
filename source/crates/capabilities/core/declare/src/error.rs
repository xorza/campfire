use std::any::Any;
use std::error::Error;
use std::fmt;

use campfire_common::DeclaredName;
use campfire_script::UnknownName;
use campfire_sim::Capability;
use campfire_values::{PackageIndex, PackagePath};
use thiserror::Error;

/// What the package load did not check and a match's books cannot hold: each in the package at
/// its place among the match's packages, the mode's 0.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BookError {
    /// Data at `at` names what the mode or its packages do not have.
    #[error("{at}")]
    Unknown {
        package: PackageIndex,
        at: BookPlace,
        #[source]
        name: UnknownName,
    },
    /// Data names a script at `path` that the package does not hold.
    #[error("no script {path}")]
    Script {
        package: PackageIndex,
        path: PackagePath,
    },
    /// Data at `at` uses `capability`, which the mode does not declare.
    #[error("{at} uses {capability:?}, which the mode does not declare")]
    Undeclared {
        package: PackageIndex,
        at: BookPlace,
        capability: Capability,
    },
    /// Data at `at` breaks a rule of the module that reads it.
    #[error("{at}")]
    Rule {
        package: PackageIndex,
        at: BookPlace,
        #[source]
        problem: Problem,
    },
}

impl BookError {
    /// The error of `problem`, a rule of a module that data at `at` of the package at `package`
    /// breaks.
    pub fn rule<E: Error + Clone + PartialEq + Send + Sync + 'static>(
        package: PackageIndex,
        at: BookPlace,
        problem: E,
    ) -> Self {
        Self::Rule {
            package,
            at,
            problem: Problem(Box::new(problem)),
        }
    }

    /// The error of `name`, which data at `at` of the package at `package` names.
    pub const fn unknown(package: PackageIndex, at: BookPlace, name: UnknownName) -> Self {
        Self::Unknown { package, at, name }
    }
}

/// Why a list of names the mode declares, as its pools or its players' resources, does not load.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NamesListError {
    /// More names than the ids of their kind count.
    #[error("more names than their ids count")]
    TooMany,
    /// A name the list holds already.
    #[error("{0} twice")]
    Repeated(DeclaredName),
}

/// Where in a package a book finds what it refuses, as a book error names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookPlace {
    UnitType(TypePlace),
    /// The entry `name` of a package's table `table`, as `actions`, or of the mode's, as `pools`.
    Entry {
        table: &'static str,
        name: DeclaredName,
    },
    /// The mode's section of the key.
    Section(&'static str),
    /// The map's section of the key.
    MapSection(&'static str),
    /// The mode's map, its units and markers.
    Map,
    /// The mode's teams and their relations.
    Teams,
    /// The match's unit types and tags.
    Types,
}

/// Each place by the path a data file names it by.
impl fmt::Display for BookPlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnitType(unit_type) => write!(f, "{unit_type}"),
            Self::Entry { table, name } => write!(f, "{table}.{name}"),
            Self::Section(key) => write!(f, "the mode's [{key}]"),
            Self::MapSection(key) => write!(f, "the map's [{key}]"),
            Self::Map => f.write_str("the map"),
            Self::Teams => f.write_str("the mode's teams"),
            Self::Types => f.write_str("the match's unit types"),
        }
    }
}

/// A unit type, as a book error names it: the avatar its package stands as, or a type by the name
/// its package declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypePlace {
    Avatar,
    Declared(DeclaredName),
}

impl fmt::Display for TypePlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Avatar => f.write_str("the avatar"),
            Self::Declared(name) => write!(f, "unit type {name}"),
        }
    }
}

/// A module's own error of a rule its data breaks, whatever its type, as the module that reads the
/// data knows its rules and the layers below it do not; a caller above reads it by its type.
pub struct Problem(Box<dyn ProblemValue>);

/// A module's error as a problem holds it.
trait ProblemValue: Error + Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;

    fn clone_box(&self) -> Box<dyn ProblemValue>;

    fn eq_dyn(&self, other: &dyn ProblemValue) -> bool;
}

impl<E: Error + Clone + PartialEq + Send + Sync + 'static> ProblemValue for E {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn ProblemValue> {
        Box::new(self.clone())
    }

    fn eq_dyn(&self, other: &dyn ProblemValue) -> bool {
        other.as_any().downcast_ref::<E>() == Some(self)
    }
}

impl Problem {
    /// The module's error, if it is an `E`.
    pub fn get<E: 'static>(&self) -> Option<&E> {
        self.0.as_any().downcast_ref()
    }
}

impl Clone for Problem {
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}

impl PartialEq for Problem {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_dyn(&*other.0)
    }
}

impl Eq for Problem {}

impl fmt::Debug for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

/// The module's error's own message.
impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

/// The module's error's own source.
impl Error for Problem {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}
