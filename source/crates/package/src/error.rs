use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use campfire_capabilities::{AbilityField, DeclaredName, MapProblem, ModeError, Pools, Stat};
use campfire_content::PackagePath;
use campfire_script::ScriptError;
use campfire_sim::Capability;
use toml::de::Error as TomlError;

use crate::files::version::Version;

/// Why a package file does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug)]
pub enum ContentError {
    /// The file does not read.
    Io { path: PackagePath, error: io::Error },
    /// The data file is not TOML of the expected shape.
    Data { path: PackagePath, error: TomlError },
    /// A directory of packages, or of a package's files, does not read.
    Scan { dir: PathBuf, error: io::Error },
    /// An entry of a package is neither a file nor a directory, such as a link.
    NotAFile(PathBuf),
    /// A path in a package is not UTF-8, so no file list can name it.
    NotUtf8(PathBuf),
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentError::Io { path, error } => write!(f, "{path}: {error}"),
            ContentError::Data { path, error } => write!(f, "{path}: {error}"),
            ContentError::Scan { dir, error } => write!(f, "{}: {error}", dir.display()),
            ContentError::NotAFile(path) => {
                write!(f, "{}: neither a file nor a directory", path.display())
            }
            ContentError::NotUtf8(path) => write!(f, "{}: path is not UTF-8", path.display()),
        }
    }
}

impl Error for ContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ContentError::NotAFile(_) | ContentError::NotUtf8(_) => None,
            ContentError::Io { error, .. } | ContentError::Scan { error, .. } => Some(error),
            ContentError::Data { error, .. } => Some(error),
        }
    }
}

/// Why a store does not give the packages that session terms name.
#[derive(Debug)]
pub enum StoreError {
    /// The store holds no mode of the fingerprint the terms name.
    UnknownMode,
    /// The terms name another count of dependencies than the mode's manifest.
    DependencyCount,
    /// The store holds no package of the fingerprint the terms give the dependency of this name.
    MissingDependency(String),
    /// The packages do not load.
    Load(LoadError),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::UnknownMode => f.write_str("no mode of the session's fingerprint is held"),
            StoreError::DependencyCount => {
                f.write_str("the session names another count of dependencies than the mode")
            }
            StoreError::MissingDependency(name) => {
                write!(
                    f,
                    "no package of the fingerprint the session gives {name:?} is held"
                )
            }
            StoreError::Load(error) => write!(f, "{error}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StoreError::Load(error) => Some(error),
            _ => None,
        }
    }
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
    /// The package is not of the kind its place needs: a mode, or an avatar or loadout a mode depends
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
    /// The mode declares `combat`, and no damage kinds for its damage.
    NoDamageKinds,
    /// The mode declares `combat`, and no `attack_kind` for its attacks.
    NoAttackKind,
    /// The mode declares more damage kinds than a match tells apart.
    TooManyDamageKinds,
    /// The mode declares `vision`, and its map has no grid for sight to reveal.
    NoGrid,
    /// The mode declares `navigation`, and its map has no `[navigation]` cells to plan routes on.
    NoPathingGrid,
    /// The map cannot be walked as the mode needs.
    Map(MapProblem),
    /// An avatar's slot names an ability it does not have.
    UnknownSlot(String),
    /// An avatar's ability is in none of its slots, so it has no rank count.
    Unslotted(String),
    /// An avatar's ability is in two slots.
    RepeatedSlot(String),
    /// Two loadout packages hold a loadout entry of this id.
    RepeatedLoadout(String),
    /// The mode's teams or map name what it does not have.
    Mode(ModeError),
    /// A capability field of an ability does not hold at a rank.
    AbilityField {
        ability: String,
        field: AbilityField,
    },
    /// The mode's unit types declare more tags than a match holds.
    TooManyTags,
    /// The mode has more unit types, its avatars' among them, than a match holds.
    TooManyUnitTypes,
    /// An avatar has the name of one of the mode's unit types.
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
    /// A script reads a field or calls a method no handle, no built-in and none of its own
    /// functions or object maps has.
    UnknownMember {
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
    /// A script at `at` names a marker tag no marker of the map has.
    UnknownMarkerTag {
        at: Place,
        tag: String,
    },
    /// Data or a script at `at` names a pool the mode does not declare.
    UnknownPool {
        at: Place,
        name: String,
    },
    /// A unit type at `at` lists a pool twice.
    RepeatedPool {
        at: Place,
        name: DeclaredName,
    },
    /// Live stat changes across the mode's modifiers read each other in a loop, through these
    /// stats.
    StatLoop(Vec<Stat>),
    /// The mode declares more pools than `Pools::LIMIT`.
    TooManyPools,
    /// A unit type at `at` moves on a layer the mode does not declare.
    UnknownLayer {
        at: Place,
        layer: DeclaredName,
    },
    /// The mode declares `combat` but no `[combat] life`.
    NoLifePool,
    /// A unit type at `at` has a `combat` section but not the life pool.
    LifePoolMissing(Place),
    /// The mode declares a name twice in one of its lists.
    RepeatedName(DeclaredName),
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
    Avatar(String),
    Ability(String),
    Modifier(String),
    Script(PackagePath),
    /// The map's paths.
    Paths,
    /// The mode's `attack_kind`.
    AttackKind,
    /// The mode's `[combat]`.
    Combat,
    /// The mode's `[navigation]`.
    Navigation,
    /// The mode's pool of that name.
    Pool(DeclaredName),
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
            Place::Avatar(name) => write!(f, "avatar {name}"),
            Place::Ability(id) => write!(f, "ability {id}"),
            Place::Modifier(id) => write!(f, "modifier {id}"),
            Place::Script(path) => write!(f, "{path}"),
            Place::Paths => f.write_str("the map's paths"),
            Place::AttackKind => f.write_str("the mode's attack_kind"),
            Place::Combat => f.write_str("the mode's [combat]"),
            Place::Navigation => f.write_str("the mode's [navigation]"),
            Place::Pool(name) => write!(f, "pool {name}"),
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
            LoadProblem::TooManyTags => f.write_str("more tags than a match holds"),
            LoadProblem::TooManyUnitTypes => f.write_str("more unit types than a match holds"),
            LoadProblem::RepeatedUnitType(name) => {
                write!(f, "avatar {name:?} has the name of a unit type")
            }
            LoadProblem::NoDamageKinds => {
                f.write_str("the mode declares combat, and no damage kinds")
            }
            LoadProblem::NoAttackKind => {
                f.write_str("the mode declares combat, and no attack_kind")
            }
            LoadProblem::TooManyDamageKinds => {
                f.write_str("more damage kinds than a match tells apart")
            }
            LoadProblem::NoGrid => f.write_str("the mode declares vision, and its map has no grid"),
            LoadProblem::NoPathingGrid => {
                f.write_str("the mode declares navigation, and its map has no [navigation] cells")
            }
            LoadProblem::Map(problem) => write!(f, "{problem}"),
            LoadProblem::Unslotted(id) => write!(f, "ability {id:?} is in no slot"),
            LoadProblem::RepeatedSlot(id) => write!(f, "ability {id:?} is in two slots"),
            LoadProblem::RepeatedLoadout(id) => write!(f, "two loadout packages hold {id:?}"),
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
            LoadProblem::UnknownMember { path, name } => {
                write!(f, "{path}: .{name} is no member the script API has")
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
            LoadProblem::UnknownPool { at, name } => write!(f, "{at}: no pool {name:?}"),
            LoadProblem::UnknownMarkerTag { at, tag } => {
                write!(f, "{at}: no marker with tag {tag:?}")
            }
            LoadProblem::RepeatedPool { at, name } => write!(f, "{at}: pool {name:?} twice"),
            LoadProblem::StatLoop(stats) => {
                let names: Vec<String> = stats.iter().map(Stat::to_string).collect();
                write!(
                    f,
                    "live stat changes read each other in a loop through {}",
                    names.join(", ")
                )
            }
            LoadProblem::TooManyPools => {
                write!(f, "more than {} pools", Pools::LIMIT)
            }
            LoadProblem::UnknownLayer { at, layer } => write!(f, "{at}: no layer {layer:?}"),
            LoadProblem::NoLifePool => f.write_str("combat with no [combat] life"),
            LoadProblem::LifePoolMissing(at) => write!(f, "{at}: combat without the life pool"),
            LoadProblem::RepeatedName(name) => write!(f, "the mode declares {name:?} twice"),
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
