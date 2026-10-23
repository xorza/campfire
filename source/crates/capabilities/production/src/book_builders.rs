use campfire_actions::{ActionKind, ActionsData, LoadedAction, Targeting, TypeSlotsData};
use campfire_common::DeclaredName;
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, Section, SourceType, UnitTypeData,
};
use campfire_math::Num;
use campfire_players::ResourceBook;
use campfire_script::{Filter, UnknownName};
use campfire_sim::Capability;
use campfire_stats::{LifePool, ModifierBook, StatsData, TypePoolsData};
use campfire_units::{BodyForm, PoolId, UnitTypes};
use campfire_values::{
    ActionId, Distance, ModifierId, Named, ResourceId, Share, TimeTooLarge, TypeScope, UnitType,
};

use crate::Production;
use crate::build_book::{BuildBook, NewBuild, PlacementCheck, Style};
use crate::build_data::BuildData;
use crate::builder::Builder;
use crate::construct_data::ConstructData;
use crate::drop_off_data::DropOffData;
use crate::error::ProductionDataError;
use crate::gather_data::GatherData;
use crate::gather_spec::GatherSpec;
use crate::gatherer::Gatherer;
use crate::mode_supply_data::ModeSupplyData;
use crate::node::Node;
use crate::node_book::{NodeBook, NodeSpec};
use crate::node_data::NodeData;
use crate::placement_data::{PlacementData, PlacementRule};
use crate::producer_spec::ProducerSpec;
use crate::production_data::ProductionData;
use crate::requirement_book::RequirementBook;
use crate::requires_data::RequiredUnit;
use crate::resource_set::ResourceSet;
use crate::spawn_spec::SpawnSpec;
use crate::spawns_data::SpawnsData;
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::SupplyCostBook;
use crate::supply_data::SupplyData;
use crate::train_data::TrainData;
use crate::train_spec::TrainSpec;

impl Production {
    /// The rule of every unit type: a supply only in a mode that counts supply, and a train in its
    /// slots only with a queue to train in.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let counted = cx.mode_section::<ModeSupplyData>().is_some();
        for unit in cx.types() {
            let fail = |problem| {
                let at = BookPlace::UnitType(unit.place());
                Err(BookError::rule(unit.package, at, problem))
            };
            if unit.data.section::<SupplyData>().is_some() && !counted {
                return fail(ProductionDataError::SupplyUncounted);
            }
            if unit.data.section::<ProductionData>().is_some() {
                continue;
            }
            let actions = cx.package(unit.package).content.get::<ActionsData>();
            let slots = unit.data.section::<TypeSlotsData>().into_iter();
            let mut slotted = slots.flat_map(|slots| slots.0.values()).flatten();
            let trains = slotted.find(|id| {
                let action = actions.and_then(|actions| actions.0.get(*id));
                action.is_some_and(|action| action.kind() == ActionKind::Train)
            });
            if let Some(id) = trains {
                return fail(ProductionDataError::NoQueue(id.clone()));
            }
        }
        Ok(())
    }

    /// The rule of every package's trains, builds, gathers and requirements: a train aims at
    /// nothing and makes a unit type of its package that walks, no delivery type; a build aims
    /// at a point; a gather at a unit; and an action requires no delivery type, which no player
    /// owns.
    pub(crate) fn actions_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let Some(actions) = source.content.get::<ActionsData>() else {
                continue;
            };
            for (id, action) in &actions.0 {
                let at = || BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                let fail = |problem| Err(BookError::rule(package, at(), problem));
                let train = action.section::<TrainData>();
                if let Some(train) = train {
                    if action.keys.targeting != Targeting::None {
                        return fail(ProductionDataError::TrainAims);
                    }
                    let walks = |unit: &UnitTypeData| {
                        unit.section::<StatsData>().is_some_and(StatsData::walks)
                    };
                    if let Some(unit) = source.units.get(&train.unit_type) {
                        if unit.delivers() {
                            return fail(ProductionDataError::Trained);
                        }
                        if !walks(unit) {
                            return fail(ProductionDataError::TrainStands);
                        }
                    }
                }
                let build = action.section::<BuildData>();
                if build.is_some() && action.keys.targeting != Targeting::Point {
                    return fail(ProductionDataError::BuildAims);
                }
                let gathers = action.section::<GatherData>().is_some();
                if gathers && !matches!(action.keys.targeting, Targeting::Unit(_)) {
                    return fail(ProductionDataError::GatherAims);
                }
                let requires = train.and_then(|train| train.requires.as_ref());
                let requires = requires.or_else(|| build?.requires.as_ref());
                let required = requires.iter().flat_map(|requires| &requires.units);
                let required = required.flat_map(RequiredUnit::names);
                let mut delivered = required
                    .filter(|name| source.units.get(*name).is_some_and(UnitTypeData::delivers));
                if let Some(name) = delivered.next() {
                    let unknown = UnknownName::new(UnitType::NAMES, name);
                    return Err(BookError::unknown(package, at(), unknown));
                }
            }
        }
        Ok(())
    }

    /// Tags each node type that stands `node`, and each drop-off type `drop_off`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn tag_types(cx: &BookCx<'_>, types: &mut UnitTypes) -> Result<(), BookError> {
        for unit in cx.standing() {
            if unit.data.section::<NodeData>().is_some() {
                types.give_tag(unit.unit_type, Self::NODE.tag());
            }
            if unit.data.section::<DropOffData>().is_some() {
                types.give_tag(unit.unit_type, Self::DROP_OFF.tag());
            }
        }
        Ok(())
    }

    /// The resource `name` of `names`, the players' resources when the match has them; an error
    /// for one the mode does not declare.
    fn resource(
        names: Option<&ResourceBook>,
        name: &DeclaredName,
    ) -> Result<ResourceId, UnknownName> {
        let found = names.and_then(|names| names.0.named(name.as_str()));
        found.ok_or_else(|| UnknownName::new(ResourceId::NAMES, name))
    }

    /// Whether `kind` is the kind of an action `unit` places in its slots, of its package.
    fn slots(cx: &BookCx<'_>, unit: &SourceType<'_>, kind: ActionKind) -> bool {
        let Some(slots) = unit.data.section::<TypeSlotsData>() else {
            return false;
        };
        let actions = cx.package(unit.package).content.get::<ActionsData>();
        let mut slotted = slots.0.values().flatten();
        slotted.any(|id| {
            let action = actions.and_then(|actions| actions.0.get(id));
            action.is_some_and(|action| action.kind() == kind)
        })
    }
}

impl BuildBook {
    /// The build of each action that constructs: its style, the body of its building, a type of its
    /// package that stands, its life and refund at the start, and its placement rules; an error for
    /// a name the match does not have, a building of no body (04-capabilities/production.md, A
    /// building may stand on a circle), or a start of no life for a building that lives.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let bodies = cx.book::<ById<UnitType, BodyForm>>()?;
        let mut book = Self::default();
        for action in LoadedAction::all(cx)? {
            let Some(data) = action.section::<BuildData>() else {
                continue;
            };
            let check = |rule: &PlacementRule| {
                Ok(PlacementCheck {
                    filter: Filter::resolve(&rule.filter, tags)?,
                    distance: rule.distance.get(),
                })
            };
            let rules = |of: fn(&PlacementData) -> &[PlacementRule]| {
                let placement = data.placement.as_ref();
                placement.map_or_else(
                    || Ok(Vec::new()),
                    |placement| of(placement).iter().map(check).collect(),
                )
            };
            let style = match &data.construct {
                ConstructData::Alone => Style::Alone,
                ConstructData::Builder => Style::Builder,
                ConstructData::Builders(rates) => Style::Builders(rates),
            };
            let building = &data.unit_type;
            let scope = TypeScope::of_package(action.package);
            let unit_type = cx.unit_type(scope, building.as_str());
            let unit_type = unit_type.filter(|&unit_type| cx.type_of(unit_type).stands());
            let unit_type = unit_type
                .ok_or_else(|| action.unknown(UnknownName::new(UnitType::NAMES, building)))?;
            let form = bodies.get(unit_type).copied();
            let form = form.filter(|form| form.shape().circle_radius() != Some(Num::ZERO));
            let form = form.ok_or_else(|| action.broken(ProductionDataError::NoBody))?;
            let alive = lives(cx, unit_type)?;
            let start_life = data.start_life;
            let starts = start_life.is_some_and(|share| !share.is_zero());
            if (alive && !starts) || start_life.is_some_and(Share::is_zero) {
                return Err(action.broken(ProductionDataError::StartLife));
            }
            let near = rules(|placement| &placement.near).map_err(|name| action.unknown(name))?;
            let away = rules(|placement| &placement.away).map_err(|name| action.unknown(name))?;
            book.push(
                action.action,
                NewBuild {
                    unit_type,
                    form,
                    start: data.start,
                    style,
                    start_life,
                    refund: data.cancel_refund.unwrap_or(Share::ALL),
                    near,
                    away,
                },
            );
        }
        Ok(book)
    }
}

impl TrainSpec {
    /// What each train makes: a unit type of its package; an error for one it does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<ActionId, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for action in LoadedAction::all(cx)? {
            let Some(data) = action.section::<TrainData>() else {
                continue;
            };
            let scope = TypeScope::of_package(action.package);
            let resolve = |name: &DeclaredName| {
                let unit_type = cx.unit_type(scope, name.as_str());
                unit_type.ok_or_else(|| action.unknown(UnknownName::new(UnitType::NAMES, name)))
            };
            let spec = Self {
                unit_type: resolve(&data.unit_type)?,
            };
            book.set(action.action, spec);
        }
        Ok(book.build())
    }
}

impl GatherSpec {
    /// What each gather gathers, carries, takes at each windup, looks for another node within and
    /// stands at a drop-off for; an error for a resource the mode does not declare, a unit past
    /// the take, or a time too large to count in ticks.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<ActionId, Self>, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        let mut book = ByIdBuilder::default();
        for action in LoadedAction::all(cx)? {
            let Some(data) = action.section::<GatherData>() else {
                continue;
            };
            let resource = Production::resource(resources, &data.resource);
            let unit = data.unit.unwrap_or(data.take);
            if unit > data.take {
                return Err(action.broken(ProductionDataError::UnitPastTake));
            }
            let deliver = cx.rate().ticks(data.deliver_ms.unwrap_or(0));
            book.set(
                action.action,
                Self {
                    resource: resource.map_err(|name| action.unknown(name))?,
                    take: data.take,
                    unit,
                    bounce: data.bounce.map_or(Num::ZERO, Distance::get),
                    deliver: deliver.ok_or_else(|| action.broken(TimeTooLarge))?,
                },
            );
        }
        Ok(book.build())
    }
}

impl RequirementBook {
    /// What each train and build that requires anything requires of its caster's player: unit
    /// types of its package it owns and modifiers of its package it holds; an error for a name the
    /// package does not have.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let modifiers = cx.book_if::<ModifierBook>()?;
        let mut book = Self::default();
        for action in LoadedAction::all(cx)? {
            let trains = action
                .section::<TrainData>()
                .map(|train| train.requires.as_ref());
            let builds = action
                .section::<BuildData>()
                .map(|build| build.requires.as_ref());
            let Some(requires) = trains.or(builds).flatten() else {
                continue;
            };
            let scope = TypeScope::of_package(action.package);
            let resolve = |name: &DeclaredName| {
                let unit_type = cx.unit_type(scope, name.as_str());
                unit_type.ok_or_else(|| UnknownName::new(UnitType::NAMES, name))
            };
            let units = requires.units.iter().map(|entry| {
                let types = entry.names().iter().map(resolve);
                types.collect::<Result<Vec<UnitType>, UnknownName>>()
            });
            let units = units.collect::<Result<Vec<Vec<UnitType>>, UnknownName>>();
            let units = units.map_err(|name| action.unknown(name))?;
            let held = requires.modifiers.iter().map(|name| {
                let modifier = modifiers
                    .and_then(|modifiers| modifiers.names().named((action.package, name.as_str())));
                modifier.ok_or_else(|| UnknownName::new(ModifierId::NAMES, name))
            });
            let held = held.collect::<Result<Vec<ModifierId>, UnknownName>>();
            let held = held.map_err(|name| action.unknown(name))?;
            book.push(action.action, units, held);
        }
        Ok(book)
    }
}

impl NodeBook {
    /// The resource each node type that stands holds and what becomes of its node once it runs
    /// out, and the resources each drop-off type takes; an error for a resource the mode does not
    /// declare.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        let mut nodes = ByIdBuilder::default();
        let mut drop_offs = ByIdBuilder::default();
        for unit in cx.standing() {
            let unknown =
                |name| BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), name);
            if let Some(node) = unit.data.section::<NodeData>() {
                let resource = Production::resource(resources, &node.resource).map_err(unknown)?;
                let empty = node.empty;
                nodes.set(unit.unit_type, NodeSpec { resource, empty });
            }
            if let Some(drop_off) = unit.data.section::<DropOffData>() {
                let taken = drop_off
                    .resources
                    .iter()
                    .map(|name| Production::resource(resources, name));
                let taken = taken.collect::<Result<ResourceSet, UnknownName>>();
                drop_offs.set(unit.unit_type, taken.map_err(unknown)?);
            }
        }
        Ok(Self {
            nodes: nodes.build(),
            drop_offs: drop_offs.build(),
        })
    }
}

impl SupplyCostBook {
    /// What each train costs in supply, as the supply of its unit type gives it.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mut supplies = ByIdBuilder::default();
        for unit in cx.standing() {
            if let Some(&supply) = unit.data.section::<SupplyData>() {
                supplies.set(unit.unit_type, supply);
            }
        }
        Ok(Self::new(
            supplies.build(),
            cx.book::<ById<ActionId, TrainSpec>>()?,
        ))
    }
}

impl SupplyBook {
    /// The mode's `[supply]`, none when it counts none.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        Ok(Self(cx.mode_section::<ModeSupplyData>().copied()))
    }
}

impl ProducerSpec {
    /// The production of each unit type that stands and produces; an error for a producer that
    /// starts its gatherers in a match without navigation, which walks them, and for a free unit's
    /// type as `leaving` refuses one.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<ProductionData>() else {
                continue;
            };
            if data.gathers {
                let at = || BookPlace::UnitType(unit.place());
                cx.require(Capability::Navigation, unit.package, at)?;
            }
            let free = data.free.iter().map(|name| leaving(cx, unit, name));
            let spec = Self {
                queue: data.queue,
                exit: data.exit,
                rally: data.rally,
                gathers: data.gathers,
                free: free.collect::<Result<_, _>>()?,
            };
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}

impl SpawnSpec {
    /// The spawns of each unit type that stands and keeps them; an error for one of no unit type
    /// or more than its turn counts, for a unit type as `leaving` refuses one, for one of no
    /// life, which would never die to come back, and for a time too large to count in ticks.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<SpawnsData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let broken = |problem| BookError::rule(unit.package, at(), problem);
            let count = data.unit_types.len();
            if count == 0 || count > Self::MOST_TYPES {
                return Err(broken(ProductionDataError::SpawnTypes(count)));
            }
            let mut unit_types = Vec::with_capacity(count);
            for name in &data.unit_types {
                let spawned = leaving(cx, unit, name)?;
                if !lives(cx, spawned)? {
                    return Err(broken(ProductionDataError::SpawnUnliving));
                }
                unit_types.push(spawned);
            }
            let replace = cx.rate().ticks(data.replace_ms.into());
            let spec = Self {
                unit_types: unit_types.into(),
                count: data.count,
                replace: replace
                    .ok_or_else(|| BookError::rule(unit.package, at(), TimeTooLarge))?,
            };
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}

impl Node {
    /// What each node type that stands holds as its unit spawns.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            if let Some(node) = unit.data.section::<NodeData>() {
                book.set(unit.unit_type, Self::new(node.amount.get()));
            }
        }
        Ok(book.build())
    }
}

impl Builder {
    /// A builder of each unit type that stands and places a build in its slots.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            if Production::slots(cx, unit, ActionKind::Build) {
                book.set(unit.unit_type, Self::default());
            }
        }
        Ok(book.build())
    }
}

impl Gatherer {
    /// A gatherer of each unit type that stands and places a gather in its slots.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            if Production::slots(cx, unit, ActionKind::Gather) {
                book.set(unit.unit_type, Self::default());
            }
        }
        Ok(book.build())
    }
}

/// The unit type `name` of the package of `unit`, which leaves `unit` as a free unit or a spawn;
/// an error for a type its package does not have, one of a projectile or an area, whose units
/// only actions deliver, and one that does not walk out of its exit.
fn leaving(
    cx: &BookCx<'_>,
    unit: &SourceType<'_>,
    name: &DeclaredName,
) -> Result<UnitType, BookError> {
    let at = || BookPlace::UnitType(unit.place());
    let scope = TypeScope::of_package(unit.package);
    let Some(left) = cx.unit_type(scope, name.as_str()) else {
        let name = UnknownName::new(UnitType::NAMES, name);
        return Err(BookError::unknown(unit.package, at(), name));
    };
    let data = cx.type_of(left).data;
    let broken = |problem| Err(BookError::rule(unit.package, at(), problem));
    if data.delivers() {
        return broken(ProductionDataError::LeaverDelivered);
    }
    if !data.section::<StatsData>().is_some_and(StatsData::walks) {
        return broken(ProductionDataError::LeaverStands);
    }
    Ok(left)
}

/// Whether `unit_type` of the match `cx` loads has the life pool, so its units live and die.
fn lives(cx: &BookCx<'_>, unit_type: UnitType) -> Result<bool, BookError> {
    let life = cx.fact_if::<LifePool>()?.and_then(|life| life.0);
    let pools = cx.names_if::<PoolId>()?;
    let held = cx.type_of(unit_type).data.section::<TypePoolsData>();
    Ok(held.is_some_and(|held| {
        held.0.iter().any(|name| {
            let pool = pools.and_then(|pools| pools.named(name.as_str()));
            pool.is_some() && pool == life
        })
    }))
}
