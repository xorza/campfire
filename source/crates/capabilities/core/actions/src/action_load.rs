use std::mem;

use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, Fact, Section};
use campfire_players::ResourceBook;
use campfire_script::{Filter, NameKind, UnknownName};
use campfire_stats::{
    ModifierBook, ModifierParamTable, ParamBook, ParamTable, ParamTables, PoolId, PoolNames,
    StatBook, StatId,
};
use campfire_units::{ScriptBook, UnitTypes};
use campfire_values::{
    ActionId, DamageKind, FilterData, ModifierId, NameId, Names, PackageIndex, Stat, Tag, TrackId,
    TypeScope, UnitType,
};

use crate::action_book::{ActionBook, PackageAction};
use crate::action_data::ActionData;
use crate::action_names::ActionNames;
use crate::action_parts::ActionParts;
use crate::action_ranks::ActionRanks;
use crate::actions_data::ActionsData;
use crate::cost_target::CostTarget;
use crate::damage_kind_book::DamageKindBook;
use crate::delivery::ProjectileFlight;
use crate::effect_list_book::EffectListBook;
use crate::effect_names::EffectNames;
use crate::error::ActionError;
use crate::flights::Flights;
use crate::track_names::TrackNames;

/// Every package's actions as a match loads them: the book, each action's params, one run per
/// action, and its effect lists.
#[derive(Debug, Default)]
pub struct ActionLoad {
    pub book: ActionBook,
    pub params: ParamTable,
    pub effects: EffectListBook,
}

impl Fact for ActionLoad {}

impl ActionLoad {
    /// Every package's actions, package by package, each by its id, of the ranks its slots give
    /// it, but a rules package's, which no match loads; an error for a name the match does not
    /// have, or an action that breaks a rule.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let names = LoadNames::of(cx)?;
        let ranks = cx.fact::<ActionRanks>()?;
        let mut load = Self::default();
        for (index, package) in cx.packages() {
            let actions = package.content.get::<ActionsData>().into_iter();
            for (id, data) in actions.flat_map(|actions| &actions.0) {
                let Some(ranks) = ranks.of(index, id.as_str()) else {
                    continue;
                };
                load.action(
                    cx,
                    &names,
                    Loaded {
                        index,
                        id,
                        data,
                        ranks,
                    },
                )?;
            }
        }
        Ok(load)
    }

    /// Loads `loaded`, with its script, its params and its effect lists.
    fn action(
        &mut self,
        cx: &BookCx<'_>,
        names: &LoadNames<'_>,
        loaded: Loaded<'_>,
    ) -> Result<ActionId, BookError> {
        let Loaded {
            index,
            id,
            data,
            ranks,
        } = loaded;
        let at = || BookPlace::Entry {
            table: ActionsData::KEY,
            name: id.clone(),
        };
        let script = data.script.as_ref().map(|path| cx.script(index, path));
        let script = script.transpose()?;
        let package_names = names.of_package(index, None);
        let parts = ActionParts::of(data, index, ranks, cx.rate(), &package_names);
        let parts = parts.map_err(|error| Self::error(index, id, error))?;
        let action = self.book.load(
            names.scripts,
            PackageAction {
                package: index,
                name: id.as_str(),
                data,
                script,
                parts,
            },
        );
        let run = self.params.push(&data.params, |stat| {
            let found = names.stats.and_then(|stats| stats.named(stat));
            found.ok_or_else(|| {
                BookError::unknown(index, at(), UnknownName::new(NameKind::Stat, stat))
            })
        })?;
        debug_assert_eq!(run, action.index(), "one run of params per action");
        if !(data.on_resolve.is_empty() && data.on_hit.is_empty() && data.on_end.is_empty()) {
            let mut effects = mem::take(&mut self.effects);
            let pushed = effects.push(
                action,
                data,
                &names.of_package(index, Some((action, &self.params))),
            );
            self.effects = effects;
            pushed.map_err(|error| Self::error(index, id, error))?;
        }
        Ok(action)
    }

    /// The book error of `error`, which the data of `action` of the package at `package` gives.
    fn error(package: PackageIndex, action: &DeclaredName, error: ActionError) -> BookError {
        let at = BookPlace::Entry {
            table: ActionsData::KEY,
            name: action.clone(),
        };
        match error {
            ActionError::Problem(problem) => BookError::rule(package, at, problem),
            ActionError::Unknown(name) => BookError::unknown(package, at, name),
        }
    }

    /// Every package's actions, as their load holds them.
    pub fn book(cx: &BookCx<'_>) -> Result<ActionBook, BookError> {
        Ok(cx.fact::<Self>()?.book.clone())
    }

    /// Every action's effect lists, as their load holds them.
    pub fn effects(cx: &BookCx<'_>) -> Result<EffectListBook, BookError> {
        Ok(cx.fact::<Self>()?.effects.clone())
    }

    /// The book of every action's and modifier's params, and where each action holds the params
    /// its modifiers read of it.
    pub fn param_book(cx: &BookCx<'_>) -> Result<ParamBook, BookError> {
        let mut tables = ParamTables::default();
        if let Some(modifiers) = cx.fact_if::<ModifierParamTable>()? {
            tables.modifiers = modifiers.0.clone();
        }
        tables.actions = cx.fact::<Self>()?.params.clone();
        let none = Names::default();
        let modifiers = cx.book_if::<ModifierBook>()?;
        let appliers = modifiers.map_or(&none, |modifiers| modifiers.appliers());
        Ok(ParamBook::new(tables, appliers))
    }
}

/// An action to load: its package's place, its id, its data, and its ranks.
#[derive(Debug, Clone, Copy)]
struct Loaded<'d> {
    index: PackageIndex,
    id: &'d DeclaredName,
    data: &'d ActionData,
    ranks: u8,
}

/// The tables an action's names resolve by: the mode's stats, pools, players' resources and damage
/// kinds, the match's unit types and tags, the modifiers, the tracks and the projectiles'
/// flights, each that the match has.
#[derive(Debug)]
struct LoadNames<'c> {
    cx: &'c BookCx<'c>,
    scripts: &'c ScriptBook,
    stats: Option<&'c StatBook>,
    pools: Option<&'c Names<PoolId>>,
    resources: Option<&'c ResourceBook>,
    damage_kinds: Option<&'c DamageKindBook>,
    types: &'c UnitTypes,
    modifiers: Option<&'c ModifierBook>,
    tracks: Option<&'c TrackNames>,
    flights: Option<&'c Flights>,
}

impl<'c> LoadNames<'c> {
    fn of(cx: &'c BookCx<'c>) -> Result<Self, BookError> {
        Ok(Self {
            cx,
            scripts: cx.book::<ScriptBook>()?,
            stats: cx.book_if::<StatBook>()?,
            pools: cx.fact_if::<PoolNames>()?.map(|pools| &*pools.0),
            resources: cx.book_if::<ResourceBook>()?,
            damage_kinds: cx.book_if::<DamageKindBook>()?,
            types: cx.fact::<UnitTypes>()?,
            modifiers: cx.book_if::<ModifierBook>()?,
            tracks: cx.fact_if::<TrackNames>()?,
            flights: cx.fact_if::<Flights>()?,
        })
    }

    /// The names of an action of the package at `package`, with `action`'s params when it
    /// resolves effect lists.
    const fn of_package<'n>(
        &'n self,
        package: PackageIndex,
        action: Option<(ActionId, &'n ParamTable)>,
    ) -> PackageNames<'n, 'c> {
        PackageNames {
            names: self,
            package,
            action,
        }
    }
}

/// The names an action of one package resolves, with its params when it resolves effect lists.
#[derive(Debug, Clone, Copy)]
struct PackageNames<'n, 'c> {
    names: &'n LoadNames<'c>,
    package: PackageIndex,
    action: Option<(ActionId, &'n ParamTable)>,
}

impl ActionNames for PackageNames<'_, '_> {
    fn stat(&self, stat: &Stat) -> Result<StatId, UnknownName> {
        let found = self.names.stats.and_then(|stats| stats.named(stat));
        found.ok_or_else(|| UnknownName::new(NameKind::Stat, stat))
    }

    fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName> {
        let kinds = self.names.damage_kinds;
        let kind = kinds.and_then(|kinds| kinds.0.named(name.as_str()));
        kind.ok_or_else(|| UnknownName::new(NameKind::DamageKind, name))
    }

    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        let none = Names::default();
        let resources = self
            .names
            .resources
            .map_or(&none, |resources| &*resources.0);
        let no_pools = Names::default();
        let pools = self.names.pools.unwrap_or(&no_pools);
        CostTarget::named(pools, resources, name.as_str())
    }

    fn filter(&self, filter: &FilterData) -> Result<Filter, UnknownName> {
        Filter::resolve(filter, self.names.types.tags())
    }

    fn tag(&self, name: &DeclaredName) -> Result<Tag, UnknownName> {
        let tag = self.names.types.tags().named(name.as_str());
        tag.ok_or_else(|| UnknownName::new(NameKind::Tag, name))
    }

    fn modifier(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<ModifierId, UnknownName> {
        let modifiers = self.names.modifiers;
        let modifier =
            modifiers.and_then(|modifiers| modifiers.names().named((package, name.as_str())));
        modifier.ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
    }

    fn unit_type(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<UnitType, UnknownName> {
        let scope = TypeScope::of_package(package);
        let unit_type = self.names.cx.unit_type(scope, name.as_str());
        unit_type.ok_or_else(|| UnknownName::new(NameKind::UnitType, name))
    }

    fn flight(&self, package: PackageIndex, name: &DeclaredName) -> Option<ProjectileFlight> {
        let unit_type = self.unit_type(package, name).ok()?;
        let flights = self.names.flights?;
        flights.0.get(unit_type).copied()
    }
}

impl EffectNames for PackageNames<'_, '_> {
    fn param(&self, name: &DeclaredName) -> Result<usize, UnknownName> {
        let (action, params) = self.action.expect("an effect list is of an action");
        let param = params.named(action.index(), name.as_str());
        param.ok_or_else(|| UnknownName::new(NameKind::Param, name))
    }

    fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName> {
        ActionNames::damage_kind(self, name)
    }

    fn pool(&self, name: &DeclaredName) -> Result<PoolId, UnknownName> {
        let pool = self
            .names
            .pools
            .and_then(|pools| pools.named(name.as_str()));
        pool.ok_or_else(|| UnknownName::new(NameKind::Pool, name))
    }

    fn modifier(&self, name: &DeclaredName) -> Result<ModifierId, UnknownName> {
        ActionNames::modifier(self, self.package, name)
    }

    fn track(&self, name: &DeclaredName) -> Result<TrackId, UnknownName> {
        let tracks = self.names.tracks;
        let track = tracks.and_then(|tracks| tracks.0.named(name.as_str()));
        track.ok_or_else(|| UnknownName::new(NameKind::Track, name))
    }

    fn tag(&self, name: &DeclaredName) -> Result<Tag, UnknownName> {
        ActionNames::tag(self, name)
    }

    fn package_type(&self, name: &DeclaredName) -> Result<UnitType, UnknownName> {
        self.unit_type(self.package, name)
    }
}
