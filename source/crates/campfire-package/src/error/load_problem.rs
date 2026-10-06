use std::error::Error;
use std::fmt;

use campfire_capabilities::{
    ActionDataField, ActionError, ActionField, ActionKind, AiError, ApiVersion, DeclaredName,
    EngineTag, Hook, MapProblem, ModeError, ModifierProblem, NameKind, PackagePath, ParamProblem,
    Stat, UnitKitError,
};
use campfire_sim::Capability;

use crate::error::ContentError;
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
    /// The mode's `[players]` lets a new player take a bot's slot but no open one: taking a bot's
    /// slot is a late join, which `late_join` allows.
    BotTakeoverWithoutLateJoin,
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
    /// The data at `at` gives `field`, which design 08 plans and the release does not read yet.
    Planned { field: &'static str, at: Place },
}

impl LoadProblem {
    /// The error the problem holds, as its load error's source.
    pub(crate) fn error(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LoadProblem::Content(error) => Some(error),
            LoadProblem::Mode(error) => Some(error),
            LoadProblem::Map(problem) => Some(problem),
            LoadProblem::UnitKit { error, .. } => Some(error),
            LoadProblem::Ai { error, .. } => Some(error),
            LoadProblem::Action { error, .. } => Some(error),
            LoadProblem::Modifier { problem, .. } => Some(problem),
            LoadProblem::ModifierParam { problem, .. } => Some(problem),
            LoadProblem::Script { problem, .. } => problem.error(),
            LoadProblem::Locale { problem, .. } => problem.error(),
            LoadProblem::WrongKind
            | LoadProblem::OtherName(_)
            | LoadProblem::OtherApi(_)
            | LoadProblem::Undeclared { .. }
            | LoadProblem::NoDamageKinds
            | LoadProblem::NoGrid
            | LoadProblem::NoPathingGrid
            | LoadProblem::UnknownSlot(_)
            | LoadProblem::Unslotted(_)
            | LoadProblem::AvatarOrders
            | LoadProblem::NoQueue(_)
            | LoadProblem::ActionRanks(_)
            | LoadProblem::KindField { .. }
            | LoadProblem::AttackAims(_)
            | LoadProblem::GlobalAttack(_)
            | LoadProblem::TrainAims(_)
            | LoadProblem::ClampAims(_)
            | LoadProblem::HoldAlone(_)
            | LoadProblem::KindNotRun { .. }
            | LoadProblem::ActionField { .. }
            | LoadProblem::TooMany(_)
            | LoadProblem::LevelTracks
            | LoadProblem::RankLevels(_)
            | LoadProblem::RankCount { .. }
            | LoadProblem::Repeated { .. }
            | LoadProblem::SharedPassive { .. }
            | LoadProblem::StatLoop(_)
            | LoadProblem::Choice(_)
            | LoadProblem::Unknown { .. }
            | LoadProblem::NoLifePool
            | LoadProblem::CombatMissing(_)
            | LoadProblem::BotTakeoverWithoutLateJoin
            | LoadProblem::Delivery(_)
            | LoadProblem::Effect { .. }
            | LoadProblem::EngineTag { .. }
            | LoadProblem::Item(_)
            | LoadProblem::Planned { .. } => None,
        }
    }
}

/// The problem's own text, without the error it holds: its load error gives that as its source.
/// A problem that only holds an error has no text.
impl fmt::Display for LoadProblem {
    #[expect(
        clippy::too_many_lines,
        reason = "one arm for each problem, and the problems are many"
    )]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadProblem::Content(_) | LoadProblem::Mode(_) | LoadProblem::Map(_) => Ok(()),
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
            LoadProblem::RankCount { action, ranks } => {
                write!(
                    f,
                    "action \"{action}\": a per-rank array without {ranks} entries"
                )
            }
            LoadProblem::Script {
                path,
                problem: ScriptProblem::Compile(_),
            }
            | LoadProblem::Locale {
                path,
                problem: LocaleProblem::Parse(_),
            } => write!(f, "{path}"),
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
            LoadProblem::BotTakeoverWithoutLateJoin => {
                f.write_str("the mode's [players]: bot_takeover without late_join")
            }
            LoadProblem::Locale { path, problem } => write!(f, "{path}: {problem}"),
            LoadProblem::Item(problem) => write!(f, "{problem}"),
            LoadProblem::Planned { field, at } => {
                write!(
                    f,
                    "{at}: {field} is planned, and the release does not read it yet"
                )
            }
            LoadProblem::UnitKit { at, .. } | LoadProblem::Ai { at, .. } => write!(f, "{at}"),
            LoadProblem::Action { action, .. } => write!(f, "action \"{action}\""),
            LoadProblem::Modifier { modifier, .. } => write!(f, "modifier \"{modifier}\""),
            LoadProblem::ModifierParam {
                modifier,
                param,
                way: Some(way),
                ..
            } => write!(f, "modifier \"{modifier}\", param {param}, by {way}"),
            LoadProblem::ModifierParam {
                modifier,
                param,
                way: None,
                ..
            } => write!(f, "modifier \"{modifier}\", its own param {param}"),
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
