use campfire_capabilities::CapabilityError;
use campfire_common::{DeclaredName, MapName};
use campfire_declare::Problem;
use campfire_script::{ApiVersion, NameKind, ParamProblem};
use campfire_sim::Capability;
use campfire_stats::ModifierProblem;
use campfire_values::{EngineTag, PackagePath, Stat};
use thiserror::Error;

use crate::error::ContentError;
use crate::error::locale_problem::LocaleProblem;
use crate::error::place::Place;
use crate::error::script_problem::ScriptProblem;
use crate::error::way::Way;
use crate::files::package_name::PackageName;

/// A problem that fails a package's load, as the load's checks find them (08-script-api.md, Checks
/// at package load).
#[derive(Debug, Error)]
pub enum LoadProblem {
    /// A file does not read, or a data file does not match its schema.
    #[error(transparent)]
    Content(ContentError),
    /// The capabilities a mode's manifest declares make no set the release builds.
    #[error("the manifest's capabilities")]
    Capabilities(#[source] CapabilityError),
    /// The package is not of the kind its place needs: a mode, or an avatar or loadout a mode
    /// depends on.
    #[error("not a package of the kind its place needs")]
    WrongKind,
    /// The mode's script names a package that is no rules package the mode depends on.
    #[error("the mode's script names {0}, no rules package it depends on")]
    ScriptPackage(PackageName),
    /// The mode holds no map of the name the session gives.
    #[error("the mode has no map {0}")]
    NoMap(MapName),
    /// A directory under `map/` holds a map whose name no session can give.
    #[error(
        "map/{0}/ is no map's name: a lowercase letter, then lowercase letters, digits and underscores"
    )]
    MapDirName(String),
    /// No map was named, and the mode has none, or more than one, to choose from.
    #[error("name one of the mode's maps: {}", names(.0))]
    MapNeeded(Vec<MapName>),
    /// The dependency's package has another name than the mode gives it.
    #[error("the package is named {0:?}")]
    OtherName(PackageName),
    /// The package targets a package API version this release does not load: another major, or
    /// a newer minor.
    #[error(
        "targets package API {0}, and this release loads {major}.0 to {release}",
        major = ApiVersion::RELEASE.major,
        release = ApiVersion::RELEASE
    )]
    OtherApi(ApiVersion),
    /// Data or a script at `at` uses a capability the mode does not declare.
    #[error("{at} uses {capability:?}, which the mode does not declare")]
    Undeclared { capability: Capability, at: Place },
    /// An action that players see has no `name`: an avatar's or a loadout's, which a player
    /// casts, or a train or a build, which a client shows as its producer's button.
    #[error("action \"{0}\", which players see, has no name")]
    UnnamedAction(DeclaredName),
    /// A team of the manifest has no `[teams.<team>]` that names it.
    #[error("team \"{0}\" has no name")]
    UnnamedTeam(DeclaredName),
    /// Unit types place the action in slot kinds of other ranks.
    #[error("action \"{0}\" sits in slot kinds of other ranks")]
    ActionRanks(DeclaredName),
    /// Data at `at` breaks a rule of the module that reads it, as the module reports it.
    #[error("{at}")]
    Rule {
        at: Place,
        #[source]
        problem: Problem,
    },
    /// More packages, the mode's and those it depends on, than a package index counts.
    #[error("more packages than a package index counts")]
    TooManyPackages,
    /// The script at `path`, or the one data names there.
    #[error("{path}")]
    Script {
        path: PackagePath,
        #[source]
        problem: ScriptProblem,
    },
    /// `name` twice where `at` names each once: an avatar named as one of the mode's unit
    /// types, an entry of two loadout packages, a pool or an action twice in a unit type's pools
    /// or slots, a name twice in one of the mode's lists.
    #[error("{at}: {name:?} twice")]
    Repeated { at: Place, name: String },
    /// A modifier is the passive of two owners of its package, unit types by `passive` and
    /// actions by `passive_modifier`, which would share its hold.
    #[error("modifier {modifier} is the passive of both {} and {}", .owners[0], .owners[1])]
    SharedPassive {
        modifier: DeclaredName,
        owners: [Place; 2],
    },
    /// Live stat changes across the mode's modifiers read each other in a loop, through these
    /// stats.
    #[error("live stat changes read each other in a loop through {}", joined(.0))]
    StatLoop(Vec<Stat>),
    /// Data or a script at `at` names something of `of` the mode or its packages do not have.
    #[error("{at}: no {of} {name:?}")]
    Unknown {
        at: Place,
        name: String,
        of: NameKind,
    },
    /// A unit type or a modifier at `at` carries a tag only the engine gives.
    #[error("{at}: {:?}, a tag only the engine gives", .tag.name())]
    EngineTag { at: Place, tag: EngineTag },
    /// A modifier does not load at the fastest rate the mode allows.
    #[error("modifier \"{modifier}\"")]
    Modifier {
        modifier: DeclaredName,
        #[source]
        problem: ModifierProblem,
    },
    /// A param a modifier reads does not hold as the modifier reads it.
    #[error("modifier \"{modifier}\", {}", param_of(.param, .way.as_ref()))]
    ModifierParam {
        modifier: DeclaredName,
        param: DeclaredName,
        /// The way that applies the modifier the problem is of; none for its own param, whatever
        /// applies it.
        way: Option<Way>,
        #[source]
        problem: ParamProblem,
    },
    /// A file of human text at `path`.
    #[error("{path}")]
    Locale {
        path: PackagePath,
        #[source]
        problem: LocaleProblem,
    },
    /// The data at `at` gives `field`, which the script API plans and the release does not read
    /// yet.
    #[error("{at}: {field} is planned, and the release does not read it yet")]
    Planned { field: &'static str, at: Place },
}

/// The names of `stats`, in their order.
fn joined(stats: &[Stat]) -> String {
    let names: Vec<String> = stats.iter().map(Stat::to_string).collect();
    names.join(", ")
}

/// A modifier's param `param`, as the way `way` applies the modifier, or its own when none does.
fn param_of(param: &DeclaredName, way: Option<&Way>) -> String {
    match way {
        Some(way) => format!("param {param}, by {way}"),
        None => format!("its own param {param}"),
    }
}

/// `maps` as text, comma-separated, or `none`.
fn names(maps: &[MapName]) -> String {
    if maps.is_empty() {
        return "none".to_owned();
    }
    maps.iter()
        .map(MapName::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}
