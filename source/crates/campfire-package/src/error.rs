use std::error::Error;
use std::path::PathBuf;
use std::{fmt, io};

use crate::message_id::MessageId;
use crate::modifier_ways::Way;

use campfire_capabilities::{
    ActionDataField, ActionError, ActionField, ActionKind, ActionSlots, AiError, ApiVersion,
    DeclaredName, EngineEnum, EngineTag, Hook, MapProblem, ModeError, ModifierProblem, NameKind,
    PackagePath, ParamProblem, PlannedEffect, Pools, ResourceId, Stat, TrackId, UnitKitError,
};
use campfire_common::Fingerprint;
use campfire_script::ScriptError;
use campfire_sim::Capability;
use fluent_syntax::parser::ParserError;
use toml::de::Error as TomlError;

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
    /// A file's name in a package is no package path, as one holding `\` is not.
    NotPath(PathBuf),
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
            ContentError::NotPath(path) => {
                write!(f, "{}: no path a package can name", path.display())
            }
        }
    }
}

impl Error for ContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ContentError::NotAFile(_) | ContentError::NotUtf8(_) | ContentError::NotPath(_) => None,
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

/// A problem that fails a package's load, as design 08's checks find them.
#[derive(Debug)]
pub enum LoadProblem {
    /// A file does not read, or a data file does not match its schema.
    Content(ContentError),
    /// The package is not of the kind its place needs: a mode, or an avatar or loadout a mode
    /// depends on.
    WrongKind,
    /// The dependency's package has another name than the mode gives it.
    OtherName(String),
    /// The package targets a package API version this release does not load: another major, or
    /// a newer minor.
    OtherApi(ApiVersion),
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
    UnknownSlot(DeclaredName),
    /// An avatar's action is in none of its slots, so it has no rank count.
    Unslotted(DeclaredName),
    /// An avatar names an AI: a player controls it, and a bot plays it through player inputs.
    AvatarOrders,
    /// A unit type holds this train, and has no `production` section to queue it.
    NoQueue(DeclaredName),
    /// Unit types place the action in slot kinds of other ranks.
    ActionRanks(DeclaredName),
    /// An action without a field its kind needs, or with one its kind refuses, as the table of
    /// action fields says.
    KindField {
        action: DeclaredName,
        field: ActionDataField,
    },
    /// An attack aims at no unit.
    AttackAims(DeclaredName),
    /// An attack's range is global, not in meters.
    GlobalAttack(DeclaredName),
    /// A train aims at something: it takes no target.
    TrainAims(DeclaredName),
    /// An action clamps its aim to its range, and aims at no point.
    ClampAims(DeclaredName),
    /// An action holds a modifier, and has no toggle or channel to hold it while.
    HoldAlone(DeclaredName),
    /// An action of a kind the release does not run yet.
    KindNotRun {
        action: DeclaredName,
        kind: ActionKind,
    },
    /// The mode's teams or map name what it does not have.
    Mode(ModeError),
    /// A capability field of an ability does not hold at a rank.
    ActionField {
        action: DeclaredName,
        field: ActionField,
    },
    /// The mode declares more of something than a match holds.
    TooMany(Limit),
    /// More than one of the mode's tracks is the `level` track.
    LevelTracks,
    /// The slot kind of that name gives its ranks levels, in a mode with no `level` track.
    RankLevels(DeclaredName),
    /// A per-rank array of an ability has another length than its ranks.
    RankCount { action: DeclaredName, ranks: u8 },
    /// The script at `path`, or the one data names there.
    Script {
        path: PackagePath,
        problem: ScriptProblem,
    },
    /// `name` twice where `at` names each once: an avatar named as one of the mode's unit
    /// types, an entry of two loadout packages, a pool or an action twice in a unit type's pools
    /// or slots, a name twice in one of the mode's lists.
    Repeated { at: Place, name: String },
    /// A modifier is the passive of two owners of its package, unit types by `passive` and
    /// actions by `passive_modifier`, which would share its hold.
    SharedPassive {
        modifier: DeclaredName,
        owners: [Place; 2],
    },
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
    /// A unit type at `at` has the life pool but no `combat` section, so it could reach zero
    /// life and never die.
    CombatMissing(Place),
    /// A projectile or an area type, or what delivers or makes one.
    Delivery(DeliveryProblem),
    /// An effect of the action's list, the one before `list`.
    Effect {
        action: DeclaredName,
        list: Hook,
        problem: EffectProblem,
    },
    /// A unit type or a modifier at `at` carries a tag only the engine gives.
    EngineTag { at: Place, tag: EngineTag },
    /// A unit type at `at` makes no unit: its stats, pools and combat do not hold together at
    /// level 1.
    UnitKit { at: Place, error: UnitKitError },
    /// A unit type's AI at `at` does not load.
    Ai { at: Place, error: AiError },
    /// A time of an action does not count in ticks at the fastest rate the mode allows.
    Action {
        action: DeclaredName,
        error: ActionError,
    },
    /// A modifier does not load at the fastest rate the mode allows.
    Modifier {
        modifier: DeclaredName,
        problem: ModifierProblem,
    },
    /// A param a modifier reads does not hold as the modifier reads it.
    ModifierParam {
        modifier: DeclaredName,
        param: DeclaredName,
        /// The way that applies the modifier the problem is of; none for its own param, whatever
        /// applies it.
        way: Option<Way>,
        problem: ParamProblem,
    },
    /// A file of human text at `path`.
    Locale {
        path: PackagePath,
        problem: LocaleProblem,
    },
    /// The mode's item types, its shop or an inventory.
    Item(ItemProblem),
}

/// What is wrong with the mode's item types, its shop or an inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemProblem {
    /// A package other than the mode holds item types, which no shop or inventory names.
    OutsideMode,
    /// The item is built, through its components, from itself.
    ComponentLoop(DeclaredName),
    /// An inventory at `at` fills a slot kind with ranks, while an item's action has one.
    RankedInventory { at: Place, kind: DeclaredName },
    /// A unit type at `at` holds more slots, its own and its inventory's, than a unit holds.
    TooManySlots(Place),
    /// The item costs less, in some resource, than the components it is built from.
    CheaperThanComponents(DeclaredName),
    /// The shop sells the item, which costs in a resource the shop does not take.
    ShopResource(DeclaredName),
    /// The marker has the shop's tag, and no region or no team to serve.
    ShopMarker(DeclaredName),
}

impl fmt::Display for ItemProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemProblem::OutsideMode => f.write_str("only the mode package holds item types"),
            ItemProblem::ComponentLoop(item) => {
                write!(f, "item {item} is built from itself through its components")
            }
            ItemProblem::RankedInventory { at, kind } => {
                write!(
                    f,
                    "{at}: its inventory fills {kind}, a slot kind with ranks"
                )
            }
            ItemProblem::CheaperThanComponents(item) => {
                write!(f, "item {item} costs less than its components")
            }
            ItemProblem::ShopResource(item) => {
                write!(
                    f,
                    "the shop sells item {item}, which costs in another resource"
                )
            }
            ItemProblem::ShopMarker(marker) => {
                write!(
                    f,
                    "marker {marker} has the shop's tag, and no region or no team"
                )
            }
            ItemProblem::TooManySlots(at) => write!(
                f,
                "{at}: more than {} slots with its inventory",
                ActionSlots::LIMIT
            ),
        }
    }
}

/// What is wrong with a file of human text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleProblem {
    /// Its name is not `<language>.ftl`, or in a locale package `<package>/<language>.ftl` of a
    /// package it depends on, the language an identifier in its canonical spelling.
    FileName,
    /// It does not parse as Fluent, first at this error.
    Parse(ParserError),
    /// It defines the message twice.
    Repeated(MessageId),
    /// A translation defines the message, which the file of its package's own language does not.
    Stray(MessageId),
}

/// What is wrong with a script.
#[derive(Debug)]
pub enum ScriptProblem {
    /// No data names it.
    Unreferenced,
    /// Data names it, and the package does not hold it.
    Missing,
    /// It does not compile.
    Compile(ScriptError),
    /// A function named like a hook is no hook of a role the script serves, or takes another
    /// count of parameters.
    UnknownHook(String),
    /// It uses a name on `ctx` that the script API does not define, or not for its role, or not
    /// in the way it uses it.
    UnknownCtx(String),
    /// It reads a field or calls a method no handle, no built-in and none of its own functions
    /// or object maps has.
    UnknownMember(String),
    /// It reads or writes, after `.state`, a field that no state of the match declares: the
    /// mode's, a modifier's or a unit type's.
    UnknownState(String),
    /// It gives a string literal to an argument of `call` that takes a member of `takes`.
    EnumString { call: String, takes: EngineEnum },
    /// It reads or calls `path`, which is neither a member of `of` nor its function.
    UnknownEnumMember { path: String, of: EngineEnum },
    /// It makes a function pointer: a closure, an anonymous function or a call of `Fn`.
    FunctionPointer,
    /// It uses `ctx` other than design 08's convention allows, so the load checks cannot see
    /// every use of it.
    CtxMisuse(CtxMisuse),
}

/// What is wrong with an effect of an action's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectProblem {
    /// An effect the release does not run yet.
    Planned(PlannedEffect),
    /// A list of a delivery's hit or end on an action that delivers nothing, which never runs.
    NoDelivery,
    /// An effect to the unit reached in a list that reaches none: `on_end`, or `on_resolve` of
    /// an action that aims at no unit.
    NoUnit,
    /// A number below zero, at some rank.
    Negative,
    /// A number past what a sim number holds, at some rank.
    Overflow,
    /// A modifier's duration that is not a whole number of milliseconds within a `u32` at each
    /// rank, as a scaling param is not.
    Duration,
}

impl fmt::Display for EffectProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EffectProblem::Planned(effect) => {
                write!(
                    f,
                    "`{}` is an effect the release does not run yet",
                    effect.name()
                )
            }
            EffectProblem::NoDelivery => f.write_str("the action delivers nothing to hit or end"),
            EffectProblem::NoUnit => {
                f.write_str("an effect to the unit reached, where the list reaches none")
            }
            EffectProblem::Negative => f.write_str("a number below zero"),
            EffectProblem::Overflow => f.write_str("a number past a sim number"),
            EffectProblem::Duration => f.write_str(
                "a duration that is no whole number of milliseconds within a u32 at each rank",
            ),
        }
    }
}

/// What is wrong with a projectile or an area type, or with what delivers or makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryProblem {
    /// A homing projectile type at `at` is no faster than the move speed cap, so it might never
    /// catch its target.
    NotFaster(Place),
    /// A unit type at `at` with a `projectile` or an `area` section has both, or a section of a
    /// unit that stands but `vision`; a dependency's unit type is no delivery type; or an avatar is one.
    NotDelivery(Place),
    /// An action's `delivery` names a unit type of its package with no section of its kind.
    WrongSection {
        action: DeclaredName,
        unit_type: DeclaredName,
    },
    /// An action that aims at nothing delivers a projectile, which has no way to fly.
    NoAim(DeclaredName),
    /// An action's projectile homes, and the action aims at no unit, or launches more than one.
    Homing(DeclaredName),
    /// An action that aims along a direction delivers an area, which lands on a point.
    AreaDirection(DeclaredName),
    /// A weapon's delivery is no homing projectile.
    Weapon(DeclaredName),
    /// An area type has a time that does not count in ticks at the fastest rate the mode allows.
    AreaTime(DeclaredName),
    /// A train makes a projectile or an area type, whose units only actions deliver.
    Trained(DeclaredName),
    /// A projectile type at `at` hits nothing, beside a width, a stop at its first hit, a hit once
    /// a cast or homing, which only hits use.
    HitsNothing(Place),
    /// An action has an `on_hit` list or hook, and its projectile hits nothing.
    NoHit(DeclaredName),
}

impl fmt::Display for DeliveryProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeliveryProblem::NotFaster(at) => {
                write!(
                    f,
                    "{at}: homing projectile no faster than the move speed cap"
                )
            }
            DeliveryProblem::NotDelivery(at) => write!(
                f,
                "{at}: a projectile or area type has tags, params, one of the two sections and a \
                 vision section alone, a dependency's unit types are delivery types, and an \
                 avatar is none"
            ),
            DeliveryProblem::WrongSection { action, unit_type } => write!(
                f,
                "action \"{action}\" delivers unit type \"{unit_type}\", which has no section of \
                 its delivery's kind"
            ),
            DeliveryProblem::NoAim(action) => {
                write!(
                    f,
                    "action \"{action}\" aims at nothing, and a projectile needs an aim"
                )
            }
            DeliveryProblem::Homing(action) => write!(
                f,
                "action \"{action}\": a homing projectile flies one at a time, at a unit target"
            ),
            DeliveryProblem::AreaDirection(action) => write!(
                f,
                "action \"{action}\": an area lands on a point, a unit or the caster, not along a \
                 direction"
            ),
            DeliveryProblem::Weapon(action) => {
                write!(
                    f,
                    "weapon \"{action}\": its delivery is a homing projectile"
                )
            }
            DeliveryProblem::Trained(action) => {
                write!(f, "train \"{action}\" makes a projectile or an area type")
            }
            DeliveryProblem::HitsNothing(at) => write!(
                f,
                "{at}: a projectile that hits nothing has no width, no stop on hit, no hit once a \
                 cast, and does not home"
            ),
            DeliveryProblem::NoHit(action) => write!(
                f,
                "action \"{action}\" has an `on_hit`, and its projectile hits nothing"
            ),
            DeliveryProblem::AreaTime(unit_type) => {
                write!(
                    f,
                    "unit type {unit_type}: a time too large to count in ticks"
                )
            }
        }
    }
}

/// Where in a package a load problem is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    UnitType(DeclaredName),
    /// By its package's name.
    Avatar(String),
    Action(DeclaredName),
    Modifier(DeclaredName),
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
    /// The mode's unit types and its avatars, each by its name.
    UnitTypes,
    /// The actions of the mode's loadout packages.
    Loadouts,
    /// The mode's players' resources, beside its pools.
    Resources,
    /// The mode's slot kinds.
    SlotKinds,
    /// The mode's item type of that id.
    Item(DeclaredName),
    /// The mode's `[shop]`.
    Shop,
}

/// A use of `ctx` that hides it from the load checks: every value of `ctx` in a script is a
/// variable named `ctx`, used as `ctx.<name>` or as a whole argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtxMisuse {
    /// `ctx` is used other than as `ctx.<name>` or as a whole argument of a call, or is given to
    /// an operator.
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
            Place::UnitTypes => f.write_str("the mode's unit types and avatars"),
            Place::Loadouts => f.write_str("the mode's loadouts"),
            Place::Resources => f.write_str("the mode's resources and pools"),
            Place::SlotKinds => f.write_str("the mode's slot kinds"),
            Place::Item(id) => write!(f, "item {id}"),
            Place::Shop => f.write_str("the mode's [shop]"),
        }
    }
}

impl fmt::Display for ScriptProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptProblem::Unreferenced => f.write_str("no data names it"),
            ScriptProblem::Missing => f.write_str("named, but not held"),
            ScriptProblem::Compile(error) => write!(f, "{error}"),
            ScriptProblem::UnknownHook(function) => {
                write!(f, "{function} is no hook of the script's roles")
            }
            ScriptProblem::UnknownCtx(name) => {
                write!(f, "ctx.{name} is not the script API's for this script")
            }
            ScriptProblem::UnknownMember(name) => {
                write!(f, ".{name} is no member the script API has")
            }
            ScriptProblem::UnknownState(name) => {
                write!(f, ".state.{name} is a field no state of the match declares")
            }
            ScriptProblem::EnumString { call, takes } => {
                write!(f, "{call} takes a member of {takes}, not a string")
            }
            ScriptProblem::UnknownEnumMember { path, of } => {
                write!(f, "{path} is no member of {of}, nor its function")
            }
            ScriptProblem::FunctionPointer => {
                f.write_str("makes a function pointer: a closure, an anonymous function or Fn")
            }
            ScriptProblem::CtxMisuse(misuse) => write!(f, "{misuse}"),
        }
    }
}

impl fmt::Display for LocaleProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocaleProblem::FileName => {
                f.write_str("not <language>.ftl, its language in its canonical spelling")
            }
            LocaleProblem::Parse(error) => write!(f, "{error}"),
            LocaleProblem::Repeated(id) => write!(f, "message {id} twice"),
            LocaleProblem::Stray(id) => {
                write!(
                    f,
                    "message {id}, which the package's own language does not define"
                )
            }
        }
    }
}

/// What a mode declares more of than a match holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// Tags, its unit types' together.
    Tags,
    /// Unit types, its avatars' and its dependencies' delivery types among them.
    UnitTypes,
    /// Tracks, more than a unit holds.
    Tracks,
    DamageKinds,
    Pools,
    Resources,
    /// Packages, the mode's and those it depends on, more than a package index counts.
    Packages,
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
            Limit::Packages => f.write_str("more packages than a package index counts"),
        }
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
    #[expect(
        clippy::too_many_lines,
        reason = "one arm for each problem, and the problems are many"
    )]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadProblem::Content(error) => write!(f, "{error}"),
            LoadProblem::WrongKind => f.write_str("not a package of the kind its place needs"),
            LoadProblem::OtherName(name) => write!(f, "the package is named {name:?}"),
            LoadProblem::OtherApi(api) => write!(
                f,
                "targets package API {api}, and this release loads {}.0 to {}",
                ApiVersion::RELEASE.major,
                ApiVersion::RELEASE
            ),
            LoadProblem::Undeclared { capability, at } => {
                write!(
                    f,
                    "{at} uses {capability:?}, which the mode does not declare"
                )
            }
            LoadProblem::UnknownSlot(id) => write!(f, "a slot names no action \"{id}\""),
            LoadProblem::ActionField { action, field } => {
                write!(
                    f,
                    "action \"{action}\": {field:?} gives no value of its kind"
                )
            }
            LoadProblem::TooMany(limit) => write!(f, "{limit}"),
            LoadProblem::LevelTracks => f.write_str("more than one `level` track"),
            LoadProblem::RankLevels(kind) => write!(
                f,
                "slot kind {kind}: its ranks' `levels` need a `level` track"
            ),
            LoadProblem::NoDamageKinds => {
                f.write_str("the mode declares combat, and no damage kinds")
            }
            LoadProblem::NoGrid => f.write_str("the mode declares vision, and its map has no grid"),
            LoadProblem::NoPathingGrid => {
                f.write_str("the mode declares navigation, and its map has no [navigation] cells")
            }
            LoadProblem::Map(problem) => write!(f, "{problem}"),
            LoadProblem::Unslotted(id) => write!(f, "action \"{id}\" is in no slot"),
            LoadProblem::AvatarOrders => f.write_str("an avatar takes no `orders`: bots play it"),
            LoadProblem::NoQueue(id) => {
                write!(f, "train \"{id}\" sits on a unit type with no queue")
            }
            LoadProblem::KindField { action, field } => write!(
                f,
                "action \"{action}\": its kind needs or refuses `{}`",
                field.name()
            ),
            LoadProblem::AttackAims(action) => {
                write!(f, "action \"{action}\": an attack aims at a unit")
            }
            LoadProblem::GlobalAttack(action) => {
                write!(f, "action \"{action}\": an attack's range is in meters")
            }
            LoadProblem::TrainAims(action) => {
                write!(f, "action \"{action}\": a train takes no target")
            }
            LoadProblem::ClampAims(action) => {
                write!(
                    f,
                    "action \"{action}\": only a point aim clamps to the range"
                )
            }
            LoadProblem::HoldAlone(action) => {
                write!(f, "action \"{action}\": a hold needs a toggle or a channel")
            }
            LoadProblem::KindNotRun { action, kind } => {
                write!(
                    f,
                    "action \"{action}\": the release does not run {kind:?} yet"
                )
            }
            LoadProblem::ActionRanks(id) => {
                write!(f, "action \"{id}\" sits in slot kinds of other ranks")
            }
            LoadProblem::Mode(error) => write!(f, "{error}"),
            LoadProblem::RankCount { action, ranks } => {
                write!(
                    f,
                    "action \"{action}\": a per-rank array without {ranks} entries"
                )
            }
            LoadProblem::Script { path, problem } => write!(f, "{path}: {problem}"),
            LoadProblem::Unknown { at, name, of } => write!(f, "{at}: no {of} {name:?}"),
            LoadProblem::Repeated { at, name } => write!(f, "{at}: {name:?} twice"),
            LoadProblem::SharedPassive {
                modifier,
                owners: [first, second],
            } => write!(
                f,
                "modifier {modifier} is the passive of both {first} and {second}"
            ),
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
            LoadProblem::CombatMissing(at) => write!(f, "{at}: the life pool without combat"),
            LoadProblem::Locale { path, problem } => write!(f, "{path}: {problem}"),
            LoadProblem::Item(problem) => write!(f, "{problem}"),
            LoadProblem::UnitKit { at, error } => write!(f, "{at}: {error}"),
            LoadProblem::Ai { at, error } => write!(f, "{at}: {error}"),
            LoadProblem::Action { action, error } => write!(f, "action \"{action}\": {error}"),
            LoadProblem::Modifier { modifier, problem } => {
                write!(f, "modifier \"{modifier}\": {problem}")
            }
            LoadProblem::ModifierParam {
                modifier,
                param,
                way: Some(way),
                problem,
            } => write!(
                f,
                "modifier \"{modifier}\", param {param}, by {way}: {problem}"
            ),
            LoadProblem::ModifierParam {
                modifier,
                param,
                way: None,
                problem,
            } => write!(
                f,
                "modifier \"{modifier}\", its own param {param}: {problem}"
            ),
            LoadProblem::EngineTag { at, tag } => {
                write!(f, "{at}: {:?}, a tag only the engine gives", tag.name())
            }
            LoadProblem::Delivery(problem) => write!(f, "{problem}"),
            LoadProblem::Effect {
                action,
                list,
                problem,
            } => write!(f, "action \"{action}\", `{}`: {problem}", list.name()),
        }
    }
}
