use campfire_actions::{ActionKind, ActionsData, Targeting, TypeSlotsData};
use campfire_common::DeclaredName;
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, ByIdBuilder, Section, SourceType, UnitTypeData,
};
use campfire_names::UnknownName;
use campfire_players::ResourceBook;
use campfire_stats::{LifePool, StatsData, TypePoolsData};
use campfire_units::{BodyForm, PoolId, UnitTypes};
use campfire_values::{Named, ResourceId, TimeTooLarge, TypeScope, UnitType};

use crate::Production;
use crate::build_data::BuildData;
use crate::drop_off_data::DropOffData;
use crate::error::ProductionDataError;
use crate::gather_data::GatherData;
use crate::mode_supply_data::ModeSupplyData;
use crate::node_data::NodeData;
use crate::production_data::ProductionData;
use crate::requires_data::RequiredUnit;
use crate::research_data::ResearchData;
use crate::spawn_spec::SpawnSpec;
use crate::spawns_data::SpawnsData;
use crate::supply_data::SupplyData;
use crate::train_data::TrainData;

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
                action.is_some_and(|action| {
                    matches!(action.kind(), ActionKind::Train | ActionKind::Research)
                })
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
                    // A leader walks by its spawns (04-capabilities/production.md, A unit may lead
                    // its spawns).
                    let walks = |unit: &UnitTypeData| {
                        let leads = unit
                            .section::<SpawnsData>()
                            .is_some_and(|spawns| spawns.led);
                        leads || unit.section::<StatsData>().is_some_and(StatsData::walks)
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
                let research = action.section::<ResearchData>();
                if research.is_some() && action.keys.targeting != Targeting::None {
                    return fail(ProductionDataError::ResearchAims);
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
                let requires = requires.or_else(|| research?.requires.as_ref());
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
    pub(crate) fn resource(
        names: Option<&ResourceBook>,
        name: &DeclaredName,
    ) -> Result<ResourceId, UnknownName> {
        let found = names.and_then(|names| names.0.named(name.as_str()));
        found.ok_or_else(|| UnknownName::new(ResourceId::NAMES, name))
    }

    /// Whether `kind` is the kind of an action `unit` places in its slots, of its package.
    pub(crate) fn slots(cx: &BookCx<'_>, unit: &SourceType<'_>, kind: ActionKind) -> bool {
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

impl SpawnSpec {
    /// The spawns of each unit type that stands and keeps them; an error for one of no unit type
    /// or more than its turn counts, for a unit type as `leaving` refuses one, for one of no
    /// life, which would never die to come back, for a time too large to count in ticks, and for
    /// a leader of a life, a body or a walk of its own (04-capabilities/production.md, A unit may
    /// lead its spawns).
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<SpawnsData>() else {
                continue;
            };
            let at = || BookPlace::UnitType(unit.place());
            let broken = |problem| BookError::rule(unit.package, at(), problem);
            if data.led {
                let bodies = cx.book::<ById<UnitType, BodyForm>>()?;
                let walks = unit
                    .data
                    .section::<StatsData>()
                    .is_some_and(StatsData::walks);
                if lives(cx, unit.unit_type)? {
                    return Err(broken(ProductionDataError::LeaderLives));
                }
                if bodies.get(unit.unit_type).is_some() {
                    return Err(broken(ProductionDataError::LeaderBody));
                }
                if walks {
                    return Err(broken(ProductionDataError::LeaderWalks));
                }
            }
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
                led: data.led,
            };
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}

/// The unit type `name` of the package of `unit`, which leaves `unit` as a free unit or a spawn;
/// an error for a type its package does not have, one of a projectile or an area, whose units
/// only actions deliver, and one that does not walk out of its exit.
pub(crate) fn leaving(
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
pub(crate) fn lives(cx: &BookCx<'_>, unit_type: UnitType) -> Result<bool, BookError> {
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
