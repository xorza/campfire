use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use campfire_capabilities::{
    ActionField, ActionKind, ActionSlots, DeclaredName, MapProblem, ModeError, Pools, ResourceId,
    Stat, TrackId,
};
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
    Undeclared { capability: Capability, at: Place },
    /// The mode declares `combat`, and no damage kinds for its damage.
    NoDamageKinds,
    /// The mode declares `vision`, and its map has no grid for sight to reveal.
    NoGrid,
    /// The mode declares `navigation`, and its map has no `[navigation]` cells to plan routes on.
    NoPathingGrid,
    /// The map cannot be walked as the mode needs.
    Map(MapProblem),
    /// A unit type's slot names an action its package does not have.
    UnknownSlot(String),
    /// An avatar's action is in none of its slots, so it has no rank count.
    Unslotted(String),
    /// An avatar names an AI: a player controls it, and a bot plays it through player inputs.
    AvatarOrders,
    /// A unit type holds this train, and has no `production` section to queue it.
    NoQueue(String),
    /// A unit type's action is in two slots.
    RepeatedSlot(String),
    /// Unit types place the action in slot kinds of other ranks.
    ActionRanks(String),
    /// A weapon without `rate`, `damage` or `damage_kind`, a unit target or a range in meters,
    /// or with a field only a cast runs; a train without its `unit_type`, or with a field it does
    /// not run; or another kind of action with one of a weapon's or a train's fields.
    KindField(String),
    /// An action of a kind the release does not run yet.
    KindNotRun { action: String, kind: ActionKind },
    /// Two loadout packages hold a loadout entry of this id.
    RepeatedLoadout(String),
    /// The mode's teams or map name what it does not have.
    Mode(ModeError),
    /// A capability field of an ability does not hold at a rank.
    ActionField { action: String, field: ActionField },
    /// The mode declares more of something than a match holds.
    TooMany(Limit),
    /// More than one of the mode's tracks is the `level` track.
    LevelTracks,
    /// An avatar has the name of one of the mode's unit types.
    RepeatedUnitType(String),
    /// A per-rank array of an ability has another length than its ranks.
    RankCount { action: String, ranks: u8 },
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
    UnknownHook { path: PackagePath, function: String },
    /// A script uses a name on `ctx` that the script API does not define, or not for its role, or
    /// not in the way it uses it.
    UnknownCtx { path: PackagePath, name: String },
    /// A script reads a field or calls a method no handle, no built-in and none of its own
    /// functions or object maps has.
    UnknownMember { path: PackagePath, name: String },
    /// A script uses `ctx` other than design 08's convention allows, so the load checks cannot
    /// see every use of it.
    CtxMisuse {
        path: PackagePath,
        misuse: CtxMisuse,
    },
    /// A param that data or a script at `at` reads is not declared.
    /// A unit type at `at` lists a pool twice.
    RepeatedPool { at: Place, name: DeclaredName },
    /// Live stat changes across the mode's modifiers read each other in a loop, through these
    /// stats.
    StatLoop(Vec<Stat>),
    /// The mode's slot kinds or choices, or what names them.
    Choice(ChoiceProblem),
    /// Data or a script at `at` names something of `of` the mode or its packages do not have.
    Unknown {
        at: Place,
        name: String,
        of: NameKind,
    },
    /// The mode declares `combat` but no `[combat] life`.
    NoLifePool,
    /// A unit type at `at` has a `combat` section but not the life pool.
    LifePoolMissing(Place),
    /// The mode declares a name twice in one of its lists.
    RepeatedName(DeclaredName),
    /// A projectile at `at` is no faster than the move speed cap, so a homing one might never
    /// catch its target.
    ProjectileNotFaster { at: Place },
    /// A field of mode state has no `sync`, or another state field has one.
    StateSync(String),
}

/// Where in a package a load problem is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    UnitType(String),
    Avatar(String),
    Action(String),
    Modifier(String),
    Script(PackagePath),
    /// The map's paths.
    Paths,
    /// The mode's `[combat]`.
    Combat,
    /// The mode's `[navigation]`.
    Navigation,
    /// The mode's pool of that name.
    Pool(DeclaredName),
    /// The mode's choice of that name.
    Choice(DeclaredName),
    /// The mode's `[tracks]`.
    Tracks,
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
            Place::Action(id) => write!(f, "action {id}"),
            Place::Modifier(id) => write!(f, "modifier {id}"),
            Place::Script(path) => write!(f, "{path}"),
            Place::Paths => f.write_str("the map's paths"),
            Place::Combat => f.write_str("the mode's [combat]"),
            Place::Navigation => f.write_str("the mode's [navigation]"),
            Place::Pool(name) => write!(f, "pool {name}"),
            Place::Choice(name) => write!(f, "choice {name}"),
            Place::Tracks => f.write_str("the mode's [tracks]"),
        }
    }
}

/// What kind of name a load did not find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Param,
    Modifier,
    Stat,
    Pool,
    Cost,
    MarkerTag,
    Resource,
    Layer,
    Filter,
    DamageKind,
    Track,
    UnitType,
}

/// What a mode declares more of than a match holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// Tags, its unit types' together.
    Tags,
    /// Unit types, its avatars' among them.
    UnitTypes,
    /// Tracks, more than a unit holds.
    Tracks,
    DamageKinds,
    Pools,
    Resources,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Limit::Tags => f.write_str("more tags than a match holds"),
            Limit::UnitTypes => f.write_str("more unit types than a match holds"),
            Limit::Tracks => write!(f, "more than {} tracks", TrackId::LIMIT),
            Limit::DamageKinds => f.write_str("more damage kinds than a match tells apart"),
            Limit::Pools => write!(f, "more than {} pools", Pools::LIMIT),
            Limit::Resources => write!(f, "more than {} player resources", ResourceId::LIMIT),
        }
    }
}

impl fmt::Display for NameKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NameKind::Param => "param",
            NameKind::Modifier => "modifier",
            NameKind::Stat => "stat",
            NameKind::Pool => "pool",
            NameKind::Cost => "pool or player resource",
            NameKind::MarkerTag => "marker with tag",
            NameKind::Resource => "player resource",
            NameKind::Layer => "layer",
            NameKind::Filter => "filter",
            NameKind::DamageKind => "damage kind",
            NameKind::Track => "track",
            NameKind::UnitType => "unit type",
        })
    }
}

/// What is wrong with the mode's slot kinds or choices, or with a name of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceProblem {
    /// The mode declares more slot kinds than `ActionSlots::LIMIT`.
    TooManySlotKinds,
    /// Data or a script at `at` names a slot kind the mode does not declare.
    UnknownSlotKind { at: Place, kind: String },
    /// A script at `at` names a choice the mode does not declare.
    UnknownChoice { at: Place, name: String },
    /// A choice of loadout entries names no slot kind, or a choice of avatars names one.
    ChoiceSlot(DeclaredName),
    /// Two choices of loadout entries fill slot kinds of other ranks.
    LoadoutRanks,
}

impl fmt::Display for ChoiceProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChoiceProblem::TooManySlotKinds => {
                write!(f, "more than {} slot kinds", ActionSlots::LIMIT)
            }
            ChoiceProblem::UnknownSlotKind { at, kind } => write!(f, "{at}: no slot kind {kind:?}"),
            ChoiceProblem::UnknownChoice { at, name } => write!(f, "{at}: no choice {name:?}"),
            ChoiceProblem::ChoiceSlot(choice) => write!(
                f,
                "choice {choice}: a choice of loadout entries fills a slot kind, one of avatars none"
            ),
            ChoiceProblem::LoadoutRanks => {
                f.write_str("choices of loadout entries fill slot kinds of other ranks")
            }
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
            LoadProblem::UnknownSlot(id) => write!(f, "a slot names no action {id:?}"),
            LoadProblem::ActionField { action, field } => {
                write!(f, "action {action:?}: {field:?} gives no value of its kind")
            }
            LoadProblem::TooMany(limit) => write!(f, "{limit}"),
            LoadProblem::LevelTracks => f.write_str("more than one `level` track"),
            LoadProblem::RepeatedUnitType(name) => {
                write!(f, "avatar {name:?} has the name of a unit type")
            }
            LoadProblem::NoDamageKinds => {
                f.write_str("the mode declares combat, and no damage kinds")
            }
            LoadProblem::NoGrid => f.write_str("the mode declares vision, and its map has no grid"),
            LoadProblem::NoPathingGrid => {
                f.write_str("the mode declares navigation, and its map has no [navigation] cells")
            }
            LoadProblem::Map(problem) => write!(f, "{problem}"),
            LoadProblem::Unslotted(id) => write!(f, "action {id:?} is in no slot"),
            LoadProblem::AvatarOrders => f.write_str("an avatar takes no `orders`: bots play it"),
            LoadProblem::NoQueue(id) => write!(f, "train {id:?} sits on a unit type with no queue"),
            LoadProblem::RepeatedSlot(id) => write!(f, "action {id:?} is in two slots"),
            LoadProblem::KindField(action) => write!(
                f,
                "action {action:?}: an attack aims at a unit within meters, with rate, damage and \
                 damage_kind and none of a cast's fields; a train names its unit_type and takes no \
                 target; and no other kind has their fields"
            ),
            LoadProblem::KindNotRun { action, kind } => {
                write!(
                    f,
                    "action {action:?}: the release does not run {kind:?} yet"
                )
            }
            LoadProblem::ActionRanks(id) => {
                write!(f, "action {id:?} sits in slot kinds of other ranks")
            }
            LoadProblem::RepeatedLoadout(id) => write!(f, "two loadout packages hold {id:?}"),
            LoadProblem::Mode(error) => write!(f, "{error}"),
            LoadProblem::RankCount { action, ranks } => {
                write!(
                    f,
                    "action {action:?}: a per-rank array without {ranks} entries"
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
            LoadProblem::Unknown { at, name, of } => write!(f, "{at}: no {of} {name:?}"),
            LoadProblem::RepeatedPool { at, name } => write!(f, "{at}: pool {name:?} twice"),
            LoadProblem::StatLoop(stats) => {
                let names: Vec<String> = stats.iter().map(Stat::to_string).collect();
                write!(
                    f,
                    "live stat changes read each other in a loop through {}",
                    names.join(", ")
                )
            }
            LoadProblem::Choice(problem) => write!(f, "{problem}"),
            LoadProblem::NoLifePool => f.write_str("combat with no [combat] life"),
            LoadProblem::LifePoolMissing(at) => write!(f, "{at}: combat without the life pool"),
            LoadProblem::RepeatedName(name) => write!(f, "the mode declares {name:?} twice"),
            LoadProblem::ProjectileNotFaster { at } => {
                write!(f, "{at}: projectile no faster than the move speed cap")
            }
            LoadProblem::StateSync(field) => write!(f, "state {field:?}: sync where it has none"),
        }
    }
}
