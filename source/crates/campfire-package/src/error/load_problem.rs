use campfire_capabilities::{
    ActionDataField, ActionError, ActionField, ActionKind, AiError, ApiVersion, DeclaredName,
    EngineTag, Hook, MapProblem, ModeError, ModifierProblem, NameKind, PackagePath, ParamProblem,
    Stat, UnitKitError,
};
use campfire_sim::Capability;
use thiserror::Error;

use crate::error::ContentError;
use crate::error::box_problem::BoxProblem;
use crate::error::choice_problem::ChoiceProblem;
use crate::error::delivery_problem::DeliveryProblem;
use crate::error::effect_problem::EffectProblem;
use crate::error::item_problem::ItemProblem;
use crate::error::limit::Limit;
use crate::error::locale_problem::LocaleProblem;
use crate::error::place::Place;
use crate::error::script_problem::ScriptProblem;
use crate::modifier_ways::Way;

/// A problem that fails a package's load, as design 08's checks find them.
#[derive(Debug, Error)]
pub enum LoadProblem {
    /// A file does not read, or a data file does not match its schema.
    #[error(transparent)]
    Content(ContentError),
    /// The package is not of the kind its place needs: a mode, or an avatar or loadout a mode
    /// depends on.
    #[error("not a package of the kind its place needs")]
    WrongKind,
    /// The dependency's package has another name than the mode gives it.
    #[error("the package is named {0:?}")]
    OtherName(String),
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
    /// The mode declares `combat`, and no damage kinds for its damage.
    #[error("the mode declares combat, and no damage kinds")]
    NoDamageKinds,
    /// The mode declares `vision`, and its map has no grid for sight to reveal.
    #[error("the mode declares vision, and its map has no grid")]
    NoGrid,
    /// The mode declares `navigation`, and its map has no `[navigation]` cells to plan routes on.
    #[error("the mode declares navigation, and its map has no [navigation] cells")]
    NoPathingGrid,
    /// The map cannot be walked as the mode needs.
    #[error(transparent)]
    Map(MapProblem),
    /// A unit type's slot names an action its package does not have.
    #[error("a slot names no action \"{0}\"")]
    UnknownSlot(DeclaredName),
    /// An avatar's action is in none of its slots, so it has no rank count.
    #[error("action \"{0}\" is in no slot")]
    Unslotted(DeclaredName),
    /// An avatar names an AI: a player controls it, and a bot plays it through player inputs.
    #[error("an avatar takes no `orders`: bots play it")]
    AvatarOrders,
    /// A unit type holds this train, and has no `production` section to queue it.
    #[error("train \"{0}\" sits on a unit type with no queue")]
    NoQueue(DeclaredName),
    /// Unit types place the action in slot kinds of other ranks.
    #[error("action \"{0}\" sits in slot kinds of other ranks")]
    ActionRanks(DeclaredName),
    /// An action without a field its kind needs, or with one its kind refuses, as the table of
    /// action fields says.
    #[error("action \"{action}\": its kind needs or refuses `{}`", .field.name())]
    KindField {
        action: DeclaredName,
        field: ActionDataField,
    },
    /// An attack aims at no unit.
    #[error("action \"{0}\": an attack aims at a unit")]
    AttackAims(DeclaredName),
    /// An attack's range is global, not in meters.
    #[error("action \"{0}\": an attack's range is in meters")]
    GlobalAttack(DeclaredName),
    /// A train aims at something: it takes no target.
    #[error("action \"{0}\": a train takes no target")]
    TrainAims(DeclaredName),
    /// A train makes a unit type that does not walk, so no trained unit leaves its producer.
    #[error("action \"{0}\": a train makes a unit type that walks")]
    TrainStands(DeclaredName),
    /// A unit type's `supply` in a mode with no `[supply]`, which counts none.
    #[error("{0}: a supply in a mode that counts none")]
    SupplyUncounted(Place),
    /// A unit type's box body that its type or its map does not let it have.
    #[error("{at}: {problem}")]
    BoxBody { at: Place, problem: BoxProblem },
    /// An action clamps its aim to its range, and aims at no point.
    #[error("action \"{0}\": only a point aim clamps to the range")]
    ClampAims(DeclaredName),
    /// An action holds a modifier, and has no toggle or channel to hold it while.
    #[error("action \"{0}\": a hold needs a toggle or a channel")]
    HoldAlone(DeclaredName),
    /// An action of a kind the release does not run yet.
    #[error("action \"{action}\": the release does not run {kind:?} yet")]
    KindNotRun {
        action: DeclaredName,
        kind: ActionKind,
    },
    /// The mode's teams or map name what it does not have.
    #[error(transparent)]
    Mode(ModeError),
    /// A capability field of an ability does not hold at a rank.
    #[error("action \"{action}\": {field:?} gives no value of its kind")]
    ActionField {
        action: DeclaredName,
        field: ActionField,
    },
    /// The mode declares more of something than a match holds.
    #[error(transparent)]
    TooMany(Limit),
    /// More than one of the mode's tracks is the `level` track.
    #[error("more than one `level` track")]
    LevelTracks,
    /// The slot kind of that name gives its ranks levels, in a mode with no `level` track.
    #[error("slot kind {0}: its ranks' `levels` need a `level` track")]
    RankLevels(DeclaredName),
    /// A per-rank array of an ability has another length than its ranks.
    #[error("action \"{action}\": a per-rank array without {ranks} entries")]
    RankCount { action: DeclaredName, ranks: u8 },
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
    /// The mode's slot kinds or choices, or what names them.
    #[error(transparent)]
    Choice(ChoiceProblem),
    /// Data or a script at `at` names something of `of` the mode or its packages do not have.
    #[error("{at}: no {of} {name:?}")]
    Unknown {
        at: Place,
        name: String,
        of: NameKind,
    },
    /// The mode declares `combat` but no `[combat] life`.
    #[error("combat with no [combat] life")]
    NoLifePool,
    /// A unit type at `at` has the life pool but no `combat` section, so it could reach zero
    /// life and never die.
    #[error("{0}: the life pool without combat")]
    CombatMissing(Place),
    /// The mode's `[players]` lets a new player take a bot's slot but no open one: taking a bot's
    /// slot is a late join, which `late_join` allows.
    #[error("the mode's [players]: bot_takeover without late_join")]
    BotTakeoverWithoutLateJoin,
    /// A projectile or an area type, or what delivers or makes one.
    #[error(transparent)]
    Delivery(DeliveryProblem),
    /// An effect of the action's list, the one before `list`.
    #[error("action \"{action}\", `{}`", .list.name())]
    Effect {
        action: DeclaredName,
        list: Hook,
        #[source]
        problem: EffectProblem,
    },
    /// A unit type or a modifier at `at` carries a tag only the engine gives.
    #[error("{at}: {:?}, a tag only the engine gives", .tag.name())]
    EngineTag { at: Place, tag: EngineTag },
    /// A unit type at `at` makes no unit: its stats, pools and combat do not hold together at
    /// level 1.
    #[error("{at}")]
    UnitKit {
        at: Place,
        #[source]
        error: UnitKitError,
    },
    /// A unit type's AI at `at` does not load.
    #[error("{at}")]
    Ai {
        at: Place,
        #[source]
        error: AiError,
    },
    /// A time of an action does not count in ticks at the fastest rate the mode allows.
    #[error("action \"{action}\"")]
    Action {
        action: DeclaredName,
        #[source]
        error: ActionError,
    },
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
    /// The mode's item types, its shop or an inventory.
    #[error(transparent)]
    Item(ItemProblem),
    /// The data at `at` gives `field`, which design 08 plans and the release does not read yet.
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
