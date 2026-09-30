use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use campfire_capabilities::{
    AbilityError, AiError, CallError, ModeError, UnitKitError, UnitTypeError,
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
    /// A unit type does not load into the match.
    UnitType(UnitTypeError),
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
    OtherEngine(String),
    /// The mode declares `mode`, which every match has.
    DeclaresMode,
    RepeatedCapability(Capability),
    /// A capability is declared without one it builds on.
    CapabilityNeeds {
        capability: Capability,
        needs: Capability,
    },
    /// Data or a script at `at` uses a capability the mode does not declare.
    Undeclared {
        capability: Capability,
        at: Place,
    },
    /// The tick rate range is empty, starts at 0, or holds no default.
    TickRange,
    /// A script pool holds less than a whole call, or the input pool less than one for each
    /// player.
    PoolTooSmall(Pool),
    /// The move speed cap is not a positive number.
    MoveSpeedCap,
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
    /// A per-rank array of an ability has another length than its ranks.
    RankCount {
        ability: String,
        ranks: usize,
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

/// A script pool of a mode's manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    PerCall,
    Input,
    Think,
    Mode,
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
            StartError::UnitType(error) => write!(f, "{error}"),
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
            StartError::UnitType(error) => Some(error),
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
                write!(f, "targets engine release {engine:?}, not this one")
            }
            LoadProblem::DeclaresMode => f.write_str("declares mode, which every match has"),
            LoadProblem::RepeatedCapability(capability) => {
                write!(f, "declares {capability:?} twice")
            }
            LoadProblem::CapabilityNeeds { capability, needs } => {
                write!(f, "declares {capability:?} without {needs:?}")
            }
            LoadProblem::Undeclared { capability, at } => {
                write!(
                    f,
                    "{at} uses {capability:?}, which the mode does not declare"
                )
            }
            LoadProblem::TickRange => {
                f.write_str("tick rate range empty, from 0, or without its default")
            }
            LoadProblem::PoolTooSmall(pool) => write!(f, "script pool {pool:?} too small"),
            LoadProblem::MoveSpeedCap => f.write_str("move speed cap is not positive"),
            LoadProblem::UnknownSlot(id) => write!(f, "slot names no ability {id:?}"),
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
