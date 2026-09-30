use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use campfire_capabilities::{
    AbilityError, AbilityField, AiError, CallError, ModeError, UnitKitError, Version,
};
use campfire_content::{ContentError, PackagePath};
use campfire_protocol::SeedError;
use campfire_script::ScriptError;
use campfire_sim::Capability;

/// Why a session log does not start a match. A published log is untrusted, and so are packages,
/// so each is an expected failure.
#[derive(Debug)]
pub enum StartError {
    /// The log gives no segment seed.
    Seed(SeedError),
    /// The tick rate the session's terms fix is outside the mode's range.
    TickRate(NonZeroU32),
    /// The terms name another engine release than this one.
    OtherRelease(String),
    /// The store holds no mode of the fingerprint the terms name.
    UnknownMode,
    /// The packages are another mode than the one the terms name.
    OtherMode,
    /// The terms name another count of dependencies than the mode's manifest.
    DependencyCount,
    /// The store holds no package of the fingerprint the terms give the dependency of this name.
    MissingDependency(String),
    /// The packages' dependencies are others than the ones the terms name.
    OtherDependencies,
    /// The packages do not load.
    Load(LoadError),
    /// A unit type's values do not make a unit.
    UnitKit {
        unit_type: String,
        error: UnitKitError,
    },
    /// A unit type's AI does not load.
    Ai { unit_type: String, error: AiError },
    /// An ability does not load.
    Ability {
        ability: String,
        error: AbilityError,
    },
    /// The mode's setup does not start a match.
    Mode(ModeError),
    /// The mode script's `on_match_start` failed.
    MatchStart(CallError),
}

/// Why a mode's packages do not load: `problem`, in the package `package`.
#[derive(Debug)]
pub struct LoadError {
    pub package: String,
    pub problem: Box<LoadProblem>,
}

/// A problem that fails a package's load, as design 08's checks find them.
#[derive(Debug)]
pub enum LoadProblem {
    /// A file does not read, or a data file does not match its schema.
    Content(ContentError),
    /// The package is not of the kind its place needs: a mode, or a hero or spells a mode depends
    /// on.
    WrongKind,
    /// The dependency's package has another name than the mode gives it.
    OtherName(String),
    /// The package targets another engine release than this one.
    OtherEngine(Version),
    /// Data or a script at `at` uses a capability the mode does not declare.
    Undeclared {
        capability: Capability,
        at: Place,
    },
    /// A hero's slot names an ability it does not have.
    UnknownSlot(String),
    /// A hero's ability is in none of its slots, so it has no rank count.
    Unslotted(String),
    /// A hero's ability is in two slots.
    RepeatedSlot(String),
    /// Two spells packages hold a spell of this id.
    RepeatedSpell(String),
    /// The mode's teams or map name what it does not have.
    Mode(ModeError),
    /// A capability field of an ability does not hold at a rank.
    AbilityField {
        ability: String,
        field: AbilityField,
    },
    /// The mode's unit types declare more tags than a match holds.
    TooManyTags,
    /// The mode has more unit types, its heroes' among them, than a match holds.
    TooManyUnitTypes,
    /// A hero has the name of one of the mode's unit types.
    RepeatedUnitType(String),
    /// A per-rank array of an ability has another length than its ranks.
    RankCount {
        ability: String,
        ranks: u8,
    },
    /// A script file no data names.
    UnreferencedScript(PackagePath),
    /// Data names a script the package does not hold.
    MissingScript(PackagePath),
    /// A script does not compile.
    Script {
        path: PackagePath,
        error: ScriptError,
    },
    /// A function named like a hook is no hook of a role the script serves, or takes another count
    /// of parameters.
    UnknownHook {
        path: PackagePath,
        function: String,
    },
    /// A script uses a name on `ctx` that the script API does not define, or not for its role, or
    /// not in the way it uses it.
    UnknownCtx {
        path: PackagePath,
        name: String,
    },
    /// A script uses `ctx` other than design 08's convention allows, so the load checks cannot
    /// see every use of it.
    CtxMisuse {
        path: PackagePath,
        misuse: CtxMisuse,
    },
    /// A param that data or a script at `at` reads is not declared.
    UnknownParam {
        at: Place,
        name: String,
    },
    UnknownModifier {
        at: Place,
        id: String,
    },
    UnknownStat {
        at: Place,
        name: String,
    },
    UnknownFilter {
        at: Place,
        filter: String,
    },
    UnknownDamageKind {
        at: Place,
        kind: String,
    },
    /// A projectile at `at` is no faster than the move speed cap, so a homing one might never
    /// catch its target.
    ProjectileNotFaster {
        at: Place,
    },
    /// A field of mode state has no `sync`, or another state field has one.
    StateSync(String),
}

/// Where in a package a load problem is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    UnitType(String),
    Hero(String),
    Ability(String),
    Modifier(String),
    Script(PackagePath),
    /// The map's lanes.
    Lanes,
}

/// A use of `ctx` that hides it from the load checks: every value of `ctx` in a script is a
/// variable named `ctx`, used as `ctx.<name>` or as a whole argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtxMisuse {
    /// `ctx` is used other than as `ctx.<name>` or as a whole argument of a call, or is given to
    /// an operator or a function pointer's `call` or `curry`.
    Stray,
    /// The script's own function receives `ctx` under another parameter name.
    Renamed { function: String },
    /// A `let`, a `const` or a `for` binds a new variable named `ctx`.
    Bound,
    /// A hook's first parameter is not named `ctx`.
    HookParam { function: String },
}

impl fmt::Display for CtxMisuse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CtxMisuse::Stray => f.write_str("ctx used other than as ctx.<name> or a call argument"),
            CtxMisuse::Renamed { function } => {
                write!(f, "{function} receives ctx under another name")
            }
            CtxMisuse::Bound => f.write_str("binds a new variable named ctx"),
            CtxMisuse::HookParam { function } => {
                write!(f, "the first parameter of {function} is not ctx")
            }
        }
    }
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::Seed(error) => write!(f, "{error}"),
            StartError::TickRate(hz) => {
                write!(f, "{hz} ticks a second is outside the mode's range")
            }
            StartError::OtherRelease(release) => {
                write!(
                    f,
                    "the session runs on engine release {release:?}, not this one"
                )
            }
            StartError::UnknownMode => f.write_str("no mode of the session's fingerprint is held"),
            StartError::OtherMode => f.write_str("the mode is not the one the session names"),
            StartError::DependencyCount => {
                f.write_str("the session names another count of dependencies than the mode")
            }
            StartError::MissingDependency(name) => {
                write!(
                    f,
                    "no package of the fingerprint the session gives {name:?} is held"
                )
            }
            StartError::OtherDependencies => {
                f.write_str("the dependencies are not the ones the session names")
            }
            StartError::Load(error) => write!(f, "{error}"),
            StartError::UnitKit { unit_type, error } => write!(f, "unit type {unit_type}: {error}"),
            StartError::Ai { unit_type, error } => write!(f, "unit type {unit_type}: {error}"),
            StartError::Ability { ability, error } => write!(f, "ability {ability}: {error}"),
            StartError::Mode(error) => write!(f, "{error}"),
            StartError::MatchStart(error) => write!(f, "the mode's start failed: {error}"),
        }
    }
}

impl Error for StartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StartError::Seed(error) => Some(error),
            StartError::Load(error) => Some(error),
            StartError::UnitKit { error, .. } => Some(error),
            StartError::Ai { error, .. } => Some(error),
            StartError::Ability { error, .. } => Some(error),
            StartError::Mode(error) => Some(error),
            StartError::MatchStart(error) => Some(error),
            _ => None,
        }
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
            LoadProblem::Script { error, .. } => Some(error),
            LoadProblem::Mode(error) => Some(error),
            _ => None,
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::UnitType(name) => write!(f, "unit type {name}"),
            Place::Hero(name) => write!(f, "hero {name}"),
            Place::Ability(id) => write!(f, "ability {id}"),
            Place::Modifier(id) => write!(f, "modifier {id}"),
            Place::Script(path) => write!(f, "{path}"),
            Place::Lanes => f.write_str("the map's lanes"),
        }
    }
}

impl fmt::Display for LoadProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadProblem::Content(error) => write!(f, "{error}"),
            LoadProblem::WrongKind => f.write_str("not a package of the kind its place needs"),
            LoadProblem::OtherName(name) => write!(f, "the package is named {name:?}"),
            LoadProblem::OtherEngine(engine) => {
                write!(f, "targets engine release {engine}, not this one")
            }
            LoadProblem::Undeclared { capability, at } => {
                write!(
                    f,
                    "{at} uses {capability:?}, which the mode does not declare"
                )
            }
            LoadProblem::UnknownSlot(id) => write!(f, "slot names no ability {id:?}"),
            LoadProblem::AbilityField { ability, field } => {
                write!(
                    f,
                    "ability {ability:?}: {field:?} gives no value of its kind"
                )
            }
            LoadProblem::TooManyTags => f.write_str("more unit tags than a match holds"),
            LoadProblem::TooManyUnitTypes => f.write_str("more unit types than a match holds"),
            LoadProblem::RepeatedUnitType(name) => {
                write!(f, "hero {name:?} has the name of a unit type")
            }
            LoadProblem::Unslotted(id) => write!(f, "ability {id:?} is in no slot"),
            LoadProblem::RepeatedSlot(id) => write!(f, "ability {id:?} is in two slots"),
            LoadProblem::RepeatedSpell(id) => write!(f, "two spells packages hold {id:?}"),
            LoadProblem::Mode(error) => write!(f, "{error}"),
            LoadProblem::RankCount { ability, ranks } => {
                write!(
                    f,
                    "ability {ability:?}: a per-rank array without {ranks} entries"
                )
            }
            LoadProblem::UnreferencedScript(path) => write!(f, "{path}: no data names it"),
            LoadProblem::MissingScript(path) => write!(f, "{path}: named, but not held"),
            LoadProblem::Script { path, error } => write!(f, "{path}: {error}"),
            LoadProblem::UnknownHook { path, function } => {
                write!(f, "{path}: {function} is no hook of the script's roles")
            }
            LoadProblem::UnknownCtx { path, name } => {
                write!(
                    f,
                    "{path}: ctx.{name} is not the script API's for this script"
                )
            }
            LoadProblem::CtxMisuse { path, misuse } => write!(f, "{path}: {misuse}"),
            LoadProblem::UnknownParam { at, name } => write!(f, "{at}: no param {name:?}"),
            LoadProblem::UnknownModifier { at, id } => write!(f, "{at}: no modifier {id:?}"),
            LoadProblem::UnknownStat { at, name } => write!(f, "{at}: no stat {name:?}"),
            LoadProblem::UnknownFilter { at, filter } => write!(f, "{at}: no filter {filter:?}"),
            LoadProblem::UnknownDamageKind { at, kind } => {
                write!(f, "{at}: no damage kind {kind:?}")
            }
            LoadProblem::ProjectileNotFaster { at } => {
                write!(f, "{at}: projectile no faster than the move speed cap")
            }
            LoadProblem::StateSync(field) => write!(f, "state {field:?}: sync where it has none"),
        }
    }
}
