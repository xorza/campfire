//! Capabilities: the mechanisms a mode combines, a crate each, on the core, and the mode above
//! them. This crate is the top of them: `release` installs every module, `books` builds a
//! match's books, and `Order` is the body of a command, by the capability that owns it. Its
//! `pub use` list is the surface the crates above read.
//!
//! A capability crate depends only on the crates of lower layers, lowest first: `state-types`
//! and `values`; `geometry` and `visibility`; the `core` (units, scripts, players, and how a
//! module declares what it adds); `stats`; `actions`; `combat`, `deliveries`, `items` and
//! `progression`; `areas`, `navigation`, `projectiles` and `vision`; `abilities` and
//! `production`; and `mode` and `orders`. `LAYERS`, in `release`'s tests, holds this list, and a
//! test fails on a dependency on the same layer or a higher one. A capability gives the script
//! view its fields of a unit through its column, and the call frame what a call reads besides
//! the core's through its part.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod books;
#[cfg(feature = "internals")]
mod fixtures;
mod order;
mod release;

pub use campfire_abilities::Abilities;
pub use campfire_actions::{
    ActionData, ActionDataField, ActionField, ActionKind, ActionProblem, ActionRange, ActionSlot,
    ActionSlots, ActionTarget, Actions, ChannelData, ChargeRule, ChargeUpData, ChargesData,
    ConstructData, CostTarget, DamageFields, DeliveryData, DoesData, EffectData, EffectRule,
    EffectTo, HealFields, KindData, LaunchFields, ModifierFields, MoveData, PlacementData,
    PlacementRule, PlannedEffect, PurgeFields, RangeField, RankChannel, RankCharges, RankFields,
    RankToggle, RequiresData, RestoreFields, SlotCharges, SlotKind, SlotKindData, SlotKinds,
    SlotRanks, SlotTime, SlotTimes, SpawnFields, Targeting, TimedSlots, Toggle, TogglePer,
    UseGroup, XpFields,
};
pub use campfire_areas::{Area, AreaData, AreaInsideData, AreaReach, Areas};
pub use campfire_combat::{
    Combat, CombatData, DeathView, Deaths, Fallen, LeechData, ModeCombatData, OnDeath,
    RecentAttack, RecentAttackers, Respawn,
};
pub use campfire_core::{
    ActionId, ApiError, ApiMember, ApiOwner, ApiVersion, Applies, Block, Body, BodyForm, BodyUnit,
    CallError, CapabilityError, CapabilitySet, CollisionData, Commanded, CtxMisuse, CtxUse,
    DashDelivery, DashTo, DataField, DataTable, Dead, DeliveryEntity, EngineTag, EntityKind,
    EnumArgs, EnumPath, EnumRecord, EnumString, Facing, ForcedMove, Forms, HeldOut,
    HeldPlayerUnits, Hook, HookStatus, KindAudience, KindVisitor, Layer, Lifespan, MapBook,
    MatchEntity, MemberKind, MemberSpec, MemberUse, ModifierId, MoveStep, NameArgs, NameKind,
    OrderUnits, OutOfPlay, Owner, ParamProblem, PathId, Player, PlayerResources, PlayerUnits,
    PlayersWorld, Relations, ResourceAmount, ResourceId, RoleSet, ScriptApi, ScriptBook,
    ScriptBudgets, ScriptFacts, ScriptFailure, ScriptFailures, ScriptFunction, ScriptLimits,
    ScriptName, ScriptRole, SpawnPoint, StateDecl, StateDefault, StateSync, StateType, StateValue,
    Status, SyncAll, SyncNone, SyncOwner, SyncTeam, SyncTo, SyncedStateDecl, Tag, TagData,
    TagProperty, TagPropertyStatus, Team, TeamSet, TrackId, TypeOrigin, TypeOriginBook, TypeScope,
    TypesError, UnitEntity, UnitFilter, UnitState, UnitType, UnitTypeCore, Units, UnitsWorld,
    UnknownName,
};
pub use campfire_geometry::{Bounds, Grid, Metric, Polygon, PolygonError};
pub use campfire_items::{
    Inventory, InventoryData, ItemData, ItemId, ItemStack, Items, ItemsAction, ShopData,
};
pub use campfire_mode::{
    BrushData, ChoiceData, GridData, HeightGrid, InputType, InputValue, Leaver, ListEntry, MapData,
    MapNavigationData, MapPoint, MarkerData, MatchEnd, MatchResult, Mode, ModeData, ModeError,
    ModeInput, ModeInputs, ModeList, ModeLoad, ModeMap, ModeNamesError, ModeParam, ModeSetup,
    ModeState, ModeSync, ModeUnits, ModeWorld, Offer, Offers, PathData, PlacedUnitData,
    PlayersData, RegionData, RelationData, SaveAsked, SaveBy, SavesData, TeamManifest, WallData,
};
pub use campfire_navigation::{
    Destination, MapProblem, ModeNavigationData, Navigation, OnPath, PathEnd, PathWalker, Paths,
    Progress, Route,
};
pub use campfire_orders::{AiData, AiError, Orders, OrdersAction};
pub use campfire_production::{
    BuildOrder, BuildTarget, Builder, DropOffData, Gatherer, ModeSupplyData, NodeData,
    PlayerSupply, Production, ProductionAction, ProductionData, Queued, Rally, RallyTarget, Site,
    SupplyData, TrainQueue,
};
pub use campfire_progression::{
    Experience, Learning, Points, Progression, ProgressionAction, Thresholds, TrackData,
    TrackError, TrackSet, TrackXp,
};
pub use campfire_projectiles::{Projectile, ProjectileData, ProjectileHits, Projectiles};
pub use campfire_state_types::{
    DataKind, Declared, NotSent, Recipients, Replication, Sending, SendingWays, SentOnChange,
    SentOnInsert, SentOnce, SentPredicted, StateTypes,
};
pub use campfire_stats::{
    AuraData, EdgeError, Level, Life, ModifierClocks, ModifierData, ModifierProblem, Modifiers,
    PlayerModifiers, PoolCost, PoolData, PoolId, PoolMeters, Pools, Reapply, StatChange, StatGraph,
    StatId, StatOp, StatRule, StatValue, Stats, StatsData, StatsWorld,
};
pub use campfire_values::{
    DamageKind, DeclaredName, DeclaredStat, EngineEnum, EngineStat, FilterData, FilterTag, NameId,
    NameKey, Names, NamesError, Number, PackagePath, Param, ParamRef, Rank, Ranked, Relation,
    RelationSet, Scalar, Scaling, Share, Speed, Stat, TimeTooLarge,
};
pub use campfire_visibility::{
    HostVisibility, ModeVisibility, OutView, Setting, SettingValue, SpectatorFog, Visibility,
    VisibilityDecodeError, VisibilityError,
};
pub use campfire_vision::{SeenBy, Sight, Vision, VisionData};

pub use crate::books::Books;
pub use crate::books::book_input::{BookInput, BookKind, BookPackage};
pub use crate::books::book_place::BookPlace;
pub use crate::books::error::{BookError, UnitStartError};
pub use crate::books::package_content::PackageContent;
pub use crate::books::type_place::TypePlace;
pub use crate::books::unit_type_data::UnitTypeData;
pub use crate::order::{Action, Order};
pub use crate::release::r#match::Match;
pub use crate::release::match_spec::MatchSpec;
pub use crate::release::{Release, r#match};

#[cfg(feature = "internals")]
pub mod internals {
    pub use campfire_core::Players;
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use campfire_core::internals::ScriptsWorldInternals;
    pub use campfire_geometry::internals::{Density, KernelScene};

    pub use crate::fixtures::abilities::AbilitiesFixtures;
    pub use crate::fixtures::actions::{ActionsFixtures, MatchNames};
    pub use crate::fixtures::areas::AreasFixtures;
    pub use crate::fixtures::navigation::NavigationFixtures;
    pub use crate::fixtures::orders::OrdersFixtures;
    pub use crate::fixtures::production::ProductionFixtures;
    pub use crate::fixtures::units::{FilterFixtures, UnitsFixtures, ViewFixtures};
    pub use crate::fixtures::vision::VisionFixtures;
}

#[cfg(feature = "internals")]
pub mod bench;
