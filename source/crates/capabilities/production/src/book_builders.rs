use campfire_actions::{
    ActionBook, ActionData, ActionKind, ActionProblem, ActionsData, ConstructData, PlacementData,
    PlacementRule, TypeSlotsData,
};
use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, ByType, Section, SourceType};
use campfire_players::ResourceBook;
use campfire_script::{Filter, NameKind, UnknownName};
use campfire_stats::{LifePool, ModifierBook, PoolNames, TypePoolsData};
use campfire_units::{BodyForm, UnitTypes};
use campfire_values::{ActionId, ModifierId, PackageIndex, ResourceId, Share, TypeScope, UnitType};

use crate::Production;
use crate::build_book::{BuildBook, NewBuild, PlacementCheck, Style};
use crate::builder::Builder;
use crate::drop_off_data::DropOffData;
use crate::error::ProductionDataError;
use crate::gatherer::Gatherer;
use crate::mode_supply_data::ModeSupplyData;
use crate::node::Node;
use crate::node_book::NodeBook;
use crate::node_data::NodeData;
use crate::production_data::ProductionData;
use crate::requirement_book::RequirementBook;
use crate::resource_set::ResourceSet;
use crate::supply_book::SupplyBook;
use crate::supply_cost_book::SupplyCostBook;
use crate::supply_data::SupplyData;

/// An action of a package as production reads it: its package's place, its id there, its id in
/// the match, and its data.
#[derive(Debug, Clone, Copy)]
struct PackageAction<'d> {
    package: PackageIndex,
    name: &'d DeclaredName,
    action: ActionId,
    data: &'d ActionData,
}

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
                action.is_some_and(|action| action.kind == ActionKind::Train)
            });
            if let Some(id) = trains {
                return fail(ProductionDataError::NoQueue(id.clone()));
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

    /// Every package's actions, in the order of their ids.
    fn actions<'c>(cx: &'c BookCx<'_>) -> Result<Vec<PackageAction<'c>>, BookError> {
        let book = cx.book::<ActionBook>()?;
        let mut actions = Vec::new();
        for (package, source) in cx.packages() {
            let data = source.content.get::<ActionsData>().into_iter();
            for (name, data) in data.flat_map(|data| &data.0) {
                let Some(action) = book.names().named((package, name.as_str())) else {
                    continue;
                };
                actions.push(PackageAction {
                    package,
                    name,
                    action,
                    data,
                });
            }
        }
        Ok(actions)
    }

    /// The error of `name`, which the action `action` names and its package does not have.
    fn unknown(action: PackageAction<'_>, name: UnknownName) -> BookError {
        let at = BookPlace::Entry {
            table: ActionsData::KEY,
            name: action.name.clone(),
        };
        BookError::unknown(action.package, at, name)
    }

    /// The resource `name` of `names`, the players' resources when the match has them; an error
    /// for one the mode does not declare.
    fn resource(
        names: Option<&ResourceBook>,
        name: &DeclaredName,
    ) -> Result<ResourceId, UnknownName> {
        let found = names.and_then(|names| names.0.named(name.as_str()));
        found.ok_or_else(|| UnknownName::new(NameKind::Resource, name))
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
            action.is_some_and(|action| action.kind == kind)
        })
    }
}

impl BuildBook {
    /// The build of each action that constructs: its style, the box body of its building, a type
    /// of its package that stands, its life and refund at the start, and its placement rules; an
    /// error for a name the match does not have, a building of no box, or a start of no life for
    /// a building that lives.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let bodies = cx.book::<ByType<BodyForm>>()?;
        let life = cx.fact_if::<LifePool>()?.and_then(|life| life.0);
        let pools = cx.fact_if::<PoolNames>()?;
        let mut book = Self::default();
        for action in Production::actions(cx)? {
            let PackageAction {
                package,
                name,
                data,
                ..
            } = action;
            let Some(construct) = &data.construct else {
                continue;
            };
            let unknown = |unknown| Production::unknown(action, unknown);
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
            let style = match construct {
                ConstructData::Alone => Style::Alone,
                ConstructData::Builder => Style::Builder,
                ConstructData::Builders(rates) => Style::Builders(rates),
            };
            let building = data.unit_type.as_ref();
            let building = building.expect("a build's kind fields hold, as its parts checked");
            let scope = TypeScope::of_package(package);
            let unit_type = cx.unit_type(scope, building.as_str());
            let unit_type = unit_type.filter(|&unit_type| cx.type_of(unit_type).stands());
            let unit_type =
                unit_type.ok_or_else(|| unknown(UnknownName::new(NameKind::UnitType, building)))?;
            let problem = |problem| {
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: name.clone(),
                };
                BookError::rule(package, at, problem)
            };
            let form = bodies.get(unit_type).copied().filter(|form| form.is_box());
            let form = form.ok_or_else(|| problem(ActionProblem::NoBox))?;
            let held = cx.type_of(unit_type).data.section::<TypePoolsData>();
            let alive = held.is_some_and(|held| {
                held.0.iter().any(|name| {
                    let pool = pools.and_then(|pools| pools.0.named(name.as_str()));
                    pool.is_some() && pool == life
                })
            });
            let start_life = data.start_life;
            let starts = start_life.is_some_and(|share| !share.is_zero());
            if (alive && !starts) || start_life.is_some_and(Share::is_zero) {
                return Err(problem(ActionProblem::StartLife));
            }
            let near = rules(|placement| &placement.near).map_err(unknown)?;
            let away = rules(|placement| &placement.away).map_err(unknown)?;
            book.push(
                action.action,
                NewBuild {
                    form,
                    start: data.start.unwrap_or_default(),
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

impl RequirementBook {
    /// What each action that requires anything requires of its caster's player: unit types of
    /// its package it owns and modifiers of its package it holds; an error for a name the package
    /// does not have.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let modifiers = cx.book_if::<ModifierBook>()?;
        let mut book = Self::default();
        for action in Production::actions(cx)? {
            let Some(requires) = &action.data.requires else {
                continue;
            };
            let scope = TypeScope::of_package(action.package);
            let units = requires.units.iter().map(|name| {
                let unit_type = cx.unit_type(scope, name.as_str());
                unit_type.ok_or_else(|| UnknownName::new(NameKind::UnitType, name))
            });
            let units = units.collect::<Result<Vec<UnitType>, UnknownName>>();
            let units = units.map_err(|name| Production::unknown(action, name))?;
            let held = requires.modifiers.iter().map(|name| {
                let modifier = modifiers
                    .and_then(|modifiers| modifiers.names().named((action.package, name.as_str())));
                modifier.ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
            });
            let held = held.collect::<Result<Vec<ModifierId>, UnknownName>>();
            let held = held.map_err(|name| Production::unknown(action, name))?;
            book.push(action.action, units, held);
        }
        Ok(book)
    }
}

impl NodeBook {
    /// The resource each node type that stands holds, and the resources each drop-off type takes;
    /// an error for a resource the mode does not declare.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        let mut book = Self::default();
        for unit in cx.standing() {
            let unknown =
                |name| BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), name);
            if let Some(node) = unit.data.section::<NodeData>() {
                let resource = Production::resource(resources, &node.resource).map_err(unknown)?;
                book.nodes.set(unit.unit_type, resource);
            }
            if let Some(drop_off) = unit.data.section::<DropOffData>() {
                let taken = drop_off
                    .resources
                    .iter()
                    .map(|name| Production::resource(resources, name));
                let taken = taken.collect::<Result<ResourceSet, UnknownName>>();
                book.drop_offs.set(unit.unit_type, taken.map_err(unknown)?);
            }
        }
        Ok(book)
    }
}

impl SupplyCostBook {
    /// What each train costs in supply, as the supply of its unit type gives it.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mut supplies = ByType::default();
        for unit in cx.standing() {
            if let Some(&supply) = unit.data.section::<SupplyData>() {
                supplies.set(unit.unit_type, supply);
            }
        }
        Ok(Self::new(supplies, cx.book::<ActionBook>()?))
    }
}

impl SupplyBook {
    /// The mode's `[supply]`, none when it counts none.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        Ok(Self(cx.mode_section::<ModeSupplyData>().copied()))
    }
}

impl ProductionData {
    /// The queue of each unit type that stands and produces.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let mut book = ByType::default();
        for unit in cx.standing() {
            if let Some(&production) = unit.data.section::<Self>() {
                book.set(unit.unit_type, production);
            }
        }
        Ok(book)
    }
}

impl Node {
    /// What each node type that stands holds as its unit spawns.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let mut book = ByType::default();
        for unit in cx.standing() {
            if let Some(node) = unit.data.section::<NodeData>() {
                book.set(unit.unit_type, Self::new(node.amount.get()));
            }
        }
        Ok(book)
    }
}

impl Builder {
    /// A builder of each unit type that stands and places a build in its slots.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let mut book = ByType::default();
        for unit in cx.standing() {
            if Production::slots(cx, unit, ActionKind::Build) {
                book.set(unit.unit_type, Self::default());
            }
        }
        Ok(book)
    }
}

impl Gatherer {
    /// A gatherer of each unit type that stands and places a gather in its slots.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let mut book = ByType::default();
        for unit in cx.standing() {
            if Production::slots(cx, unit, ActionKind::Gather) {
                book.set(unit.unit_type, Self::default());
            }
        }
        Ok(book)
    }
}
