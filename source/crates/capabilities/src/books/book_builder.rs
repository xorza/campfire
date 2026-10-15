use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;

use campfire_actions::{
    ActionData, ActionKind, ActionNames, ActionParts, ActionProblem, ActionSlots, ConstructData,
    CostTarget, EffectNames, PlacementData, PlacementRule, SlotKindBook, UseGroup,
};
use campfire_areas::{AreaError, AreaSpec, Areas};
use campfire_combat::{CombatBindings, CombatBook, CombatData, OnDeath};
use campfire_interaction::{UseData, UseSlot, UseSpec};
use campfire_items::{Inventory, ItemBook, ItemError, ItemId, ItemSpec, Shop, ShopData};
use campfire_mode::{ModeInputs, ModeLoad, ModeMap, ModeNames, ResolvedMap};
use campfire_navigation::Walker;
use campfire_orders::Ai;
use campfire_players::ResourceAmount;
use campfire_production::{
    Builder, Gatherer, NewBuild, Node, PlacementCheck, Production, ResourceSet, Style, SupplyBook,
    SupplyCostBook,
};
use campfire_progression::{TrackBook, TrackSet};
use campfire_projectiles::{ProjectileSpec, Projectiles};
use campfire_script::{Filter, NameKind, ScriptId, UnknownName};
use campfire_stats::{
    ModifierBook, ModifierError, ModifierLoad, PackageModifier, ParamBook, ParamTable, PoolBook,
    PoolId, SpecError, StatBook, StatId, TypePassive,
};
use campfire_units::{Avatar, TypesError, UnitTypeBook};
use campfire_values::{
    ActionId, DamageKind, DeclaredName, FilterData, ModifierId, Names, PackageIndex, PackagePath,
    Share, Stat, Tag, TimeTooLarge, TrackId, TypeScope, UnitType,
};

use crate::books::Books;
use crate::books::book_input::{BookInput, BookKind, BookPackage};
use crate::books::book_parts::BookParts;
use crate::books::book_place::BookPlace;
use crate::books::built_books::BuiltBooks;
use crate::books::error::BookError;
use crate::books::type_place::TypePlace;
use crate::books::unit_start::{StartOf, UnitStart};
use crate::books::unit_type_data::UnitTypeData;

/// A unit type the builder looks up by the name it declared it by.
const DECLARED: &str = "the builder declared each unit type";

/// Builds a match's books from its packages, in the order a match loads them: the tags, the
/// tracks and every package's modifiers first; then every package's unit types, by name; then
/// each package's actions and the unit types' parts, the mode's, then each dependency's.
#[derive(Debug)]
pub(crate) struct BookBuilder<'a> {
    input: &'a BookInput<'a>,
    books: BookParts,
    /// The mode's stats, for every unit type, which every stat the packages name resolves by.
    stats: StatBook,
    /// The mode's pools, each with its stats.
    pools: PoolBook,
    /// The mode's pools, players' resources, damage kinds and layers.
    names: ModeNames,
    /// The life pool, none when the mode names none.
    life: Option<PoolId>,
    /// The ranks of every loadout entry.
    loadout_ranks: u8,
    /// Where each package's scripts start among the match's.
    script_starts: Vec<usize>,
}

/// A unit type that stands, as its package gives it: its name in its scope, the scope, its file,
/// and the name its package declares it by, none for the avatar its package stands as.
#[derive(Debug, Clone, Copy)]
struct Standing<'f> {
    name: &'f str,
    scope: TypeScope,
    file: &'f UnitTypeData,
    declared: Option<&'f DeclaredName>,
}

impl<'f> Standing<'f> {
    /// The type `name` of the package at `index`: the mode's in its scope, another package's,
    /// a summon or a faction's unit, in the package's own.
    fn declared(
        index: PackageIndex,
        name: &'f DeclaredName,
        file: &'f UnitTypeData,
    ) -> Standing<'f> {
        Standing {
            name: name.as_str(),
            scope: TypeScope::of_package(index),
            file,
            declared: Some(name),
        }
    }

    /// The avatar of the package `package` names, in the mode's scope.
    const fn avatar(package: &'f str, file: &'f UnitTypeData) -> Standing<'f> {
        Standing {
            name: package,
            scope: TypeScope::Mode,
            file,
            declared: None,
        }
    }

    fn place(&self) -> TypePlace {
        self.declared
            .map_or(TypePlace::Avatar, |name| TypePlace::Declared(name.clone()))
    }

    fn at(&self) -> BookPlace {
        BookPlace::UnitType(self.place())
    }
}

/// A unit type that stands as the builder loads it: the place of its package among the match's,
/// the type as its package gives it, and its id.
#[derive(Debug, Clone, Copy)]
struct TypeAt<'f> {
    package: PackageIndex,
    unit: Standing<'f>,
    unit_type: UnitType,
}

impl TypeAt<'_> {
    /// The error of `name`, which the type names and the mode does not declare.
    fn unknown(self, name: UnknownName) -> BookError {
        BookError::unknown(self.package, self.unit.at(), name)
    }
}

impl<'a> BookBuilder<'a> {
    /// The builder of `input`'s books, its tags and unit types declared, in the order their ids
    /// follow, and its stat and pool books built for them; an error for a name the mode does not
    /// declare.
    pub(crate) fn new(input: &'a BookInput<'a>) -> Result<BookBuilder<'a>, BookError> {
        let data = input.data;
        let mut script_starts = Vec::with_capacity(input.packages.len());
        let mut start = 0;
        for package in &input.packages {
            script_starts.push(start);
            start += package.scripts.len();
        }
        let types = Books::unit_types(input.tag_names, &input.packages);
        let books = BookParts {
            types: types.map_err(BookError::Types)?,
            supply: data.supply,
            ..BookParts::default()
        };
        let names = ModeNames::of(data).map_err(BookError::Names)?;
        let life = data.combat.life_pool(&names.pools);
        let life =
            life.map_err(|name| BookError::unknown(PackageIndex::MODE, BookPlace::Combat, name))?;
        let stats = BookBuilder::stat_book(input, &books.types)?;
        let pools = PoolBook::new(&data.pools, Arc::clone(&names.pools), life, |pool, stat| {
            let at = BookPlace::Pool(pool.clone());
            BookBuilder::stat_in(&stats, PackageIndex::MODE, at, stat)
        })?;
        Ok(BookBuilder {
            input,
            stats,
            pools,
            books,
            life,
            names,
            loadout_ranks: data.loadout_ranks(),
            script_starts,
        })
    }

    /// The id of `stat`, which data at `at` of the package at `package` names; an error for a
    /// stat the mode does not declare.
    fn stat_in(
        book: &StatBook,
        package: PackageIndex,
        at: BookPlace,
        stat: &Stat,
    ) -> Result<StatId, BookError> {
        let found = book.named(stat);
        found.ok_or_else(|| BookError::unknown(package, at, UnknownName::new(NameKind::Stat, stat)))
    }

    pub(crate) fn build(mut self) -> Result<Books, BookError> {
        let input = self.input;
        if input.progression {
            self.books.tracks = TrackBook::new(&input.data.tracks).map_err(BookError::Tracks)?;
        }
        for (index, package) in (0..).map(PackageIndex::new).zip(&input.packages) {
            self.modifiers(index, package)?;
        }
        for (index, package) in (0..).map(PackageIndex::new).zip(&input.packages) {
            let units = &package.content.units;
            match package.kind {
                BookKind::Mode => {
                    let ranks = self.slotted_ranks(index, units.values())?;
                    let actions = self.actions(index, package, |id| ranks.get(id).copied())?;
                    self.books.items = self.item_book(index, package, &actions)?;
                    self.package_units(index, units, &actions)?;
                }
                BookKind::Avatar(unit) => {
                    let standing = units.values().filter(|file| !file.delivers());
                    let ranks = self.slotted_ranks(index, standing.chain([unit]))?;
                    let actions = self.actions(index, package, |id| ranks.get(id).copied())?;
                    self.package_units(index, units, &actions)?;
                    let unit = Standing::avatar(package.name, unit);
                    self.unit_type(index, unit, &actions)?;
                    self.books.units.avatars.add(package.name);
                }
                BookKind::Loadout => {
                    let ranks = self.loadout_ranks;
                    let actions = self.actions(index, package, |_| Some(ranks))?;
                    self.package_units(index, units, &actions)?;
                    for (id, action) in actions {
                        self.books.units.loadout.push(id, action);
                    }
                }
                BookKind::Rules => {}
            }
        }
        self.into_books()
    }

    /// The books the builder loaded, each in the built books, the unit types and the mode's names
    /// beside them, and the mode's map, resolved, among what its install takes; an error for what
    /// the map names that the mode does not have.
    fn into_books(self) -> Result<Books, BookError> {
        let input = self.input;
        let data = input.data;
        let resolved = self.resolve_map(&self.books.types)?;
        let mut parts = self.books;
        let stats = &self.stats;
        let bindings = CombatBindings::new(&data.combat, |stat| {
            BookBuilder::stat_in(stats, PackageIndex::MODE, BookPlace::Combat, stat)
        })?;
        let standing = parts.units.unit_types.iter();
        let bodies = standing.map(|&unit_type| (unit_type, parts.bodies.get(unit_type).copied()));
        parts.types.tag_layers(&self.names.layers, bodies);
        let mut kinds: Vec<Walker> = parts.walkers.values().copied().collect();
        kinds.sort_unstable();
        kinds.dedup();
        let ResolvedMap {
            map,
            ground,
            navigation,
            vision,
        } = resolved;
        // A window past what ticks can count covers the whole match.
        let assist_window = data.combat.assist_window_ms.map(|ms| input.rate.window(ms));
        let mut built = BuiltBooks::default();
        built.add(parts.types.state_book());
        built.add(parts.types.origin_book());
        built.add(parts.types.tag_book(&data.tags));
        built.add(input.scripts.clone());
        built.add(ground);
        built.add(self.stats);
        built.add(self.pools);
        built.add(ParamBook::new(parts.params, parts.modifiers.appliers()));
        built.add(parts.modifiers);
        built.add(SupplyCostBook::new(parts.supplies, &parts.actions));
        built.add(parts.actions);
        built.add(parts.effects);
        built.add(SlotKindBook(Arc::new(data.slots.clone())));
        built.add(CombatBook {
            bindings,
            assist_window,
        });
        built.add(parts.ais);
        built.add(parts.projectiles);
        built.add(parts.areas);
        built.add(parts.sights);
        built.add(parts.lives);
        built.add(parts.pools);
        built.add(parts.passives);
        built.add(parts.deaths);
        built.add(parts.steps);
        built.add(parts.turns);
        built.add(parts.bodies);
        built.add(parts.type_tracks);
        built.add(parts.builders);
        built.add(parts.node_starts);
        built.add(parts.gatherers);
        built.add(parts.inventories);
        built.add(parts.use_groups);
        built.add(parts.slots);
        built.add(navigation);
        built.add(parts.walkers);
        built.add(vision);
        built.add(parts.tracks);
        built.add(parts.producers);
        built.add(parts.requirements);
        built.add(parts.builds);
        built.add(parts.nodes);
        built.add(SupplyBook(parts.supply));
        built.add(parts.items);
        built.add(parts.shops);
        built.add(parts.use_specs);
        built.add(parts.use_slots);
        built.add(parts.types);
        let ModeNames {
            damage_kinds,
            resources,
            ..
        } = self.names;
        Ok(Books {
            built,
            damage_kinds,
            resources,
            spawn_tags: self.input.data.navigation.spawn_tags.clone().into(),
            inputs: ModeInputs {
                units: parts.units,
                load: ModeLoad {
                    map,
                    walkers: kinds,
                    loadout_ranks: self.loadout_ranks,
                },
            },
        })
    }

    /// The mode's map, resolved, its placed units' types among `types`; an error for what it
    /// names that the mode does not have.
    fn resolve_map(&self, types: &UnitTypeBook) -> Result<ResolvedMap, BookError> {
        let input = self.input;
        let data = input.data;
        let mode_units = &input.packages[0].content.units;
        let standing = |name: &str| {
            let stands = mode_units.get(name).is_some_and(|file| !file.delivers());
            stands.then(|| {
                types
                    .types()
                    .named((TypeScope::Mode, name))
                    .expect(DECLARED)
            })
        };
        ModeMap::resolve(
            input.map,
            input.teams,
            &data.relations,
            &self.names.layers,
            standing,
        )
        .map_err(BookError::Mode)
    }

    /// The item book of the mode package `package`, at `index`, its actions `actions`; an error
    /// for a name the mode does not have.
    fn item_book(
        &self,
        index: PackageIndex,
        package: &BookPackage<'_>,
        actions: &BTreeMap<&str, ActionId>,
    ) -> Result<ItemBook, BookError> {
        let items = &package.content.items;
        let ids: Names<ItemId> = items.keys().map(DeclaredName::as_str).collect();
        let resources = &self.names.resources;
        let names = self.names(index, None);
        let mut specs = Vec::with_capacity(items.len());
        for (name, item) in items {
            let spec = || {
                let cost = item.cost.iter().map(|(resource, &amount)| {
                    let found = resources.named(resource.as_str());
                    Ok(ResourceAmount {
                        resource: found
                            .ok_or_else(|| UnknownName::new(NameKind::Resource, resource))?,
                        amount: i64::from(amount),
                    })
                });
                let components = item.components.iter().map(|component| {
                    let found = ids.named(component.as_str());
                    found.ok_or_else(|| UnknownName::new(NameKind::Item, component))
                });
                let modifiers = item.modifiers.iter();
                let modifiers =
                    modifiers.map(|modifier| ActionNames::modifier(&names, index, modifier));
                let action = item.action.as_ref().map(|action| {
                    let found = actions.get(action.as_str()).copied();
                    found.ok_or_else(|| UnknownName::new(NameKind::Ability, action))
                });
                Ok(ItemSpec {
                    cost: cost.collect::<Result<_, UnknownName>>()?,
                    components: components.collect::<Result<_, UnknownName>>()?,
                    stack: item.stack,
                    uses: item.uses,
                    modifiers: modifiers.collect::<Result<_, UnknownName>>()?,
                    action: action.transpose()?,
                })
            };
            let at = BookPlace::Item(name.clone());
            specs.push(spec().map_err(|name| BookError::unknown(index, at, name))?);
        }
        ItemBook::new(ids, specs).map_err(|error| {
            let (ItemError::StackCost(item) | ItemError::CheaperThanComponents(item)) = error;
            let name = items
                .keys()
                .nth(item.index())
                .expect("an item of the package");
            BookError::Item {
                package: index,
                item: name.clone(),
                error,
            }
        })
    }

    /// The stat book of the mode's stats, with each unit type that stands and its `stats`
    /// section, refreshed in the input's order: each package's types that stand, in its scope,
    /// and each avatar, in the mode's; an error for a stat the mode does not declare.
    fn stat_book(input: &BookInput<'_>, types: &UnitTypeBook) -> Result<StatBook, BookError> {
        let mut book = StatBook::new(&input.data.stats, input.max_move_speed.get());
        for (index, package) in (0..).map(PackageIndex::new).zip(&input.packages) {
            let units = package.content.units.iter();
            let declared = units
                .filter(|(_, file)| !file.delivers())
                .map(|(name, file)| Standing::declared(index, name, file));
            let avatar = match package.kind {
                BookKind::Avatar(unit) => Some(Standing::avatar(package.name, unit)),
                BookKind::Mode | BookKind::Loadout | BookKind::Rules => None,
            };
            for unit in declared.chain(avatar) {
                let Some(stats) = &unit.file.stats else {
                    continue;
                };
                let unit_type = types
                    .types()
                    .named((unit.scope, unit.name))
                    .expect(DECLARED);
                book.add_type(unit_type, stats)
                    .map_err(|name| BookError::unknown(index, unit.at(), name))?;
            }
        }
        Ok(book.with_order(input.stat_order.iter().copied()))
    }

    /// Declares the unit types of the package at `index` in the order its own load reads them,
    /// which numbers them: its types by name, each a delivery type or one that stands, then an
    /// avatar package's avatar. The mode's types that stand are in the mode's scope, with the
    /// avatars; every other type is in its package's own.
    pub(super) fn declare_types(
        types: &mut UnitTypeBook,
        index: PackageIndex,
        package: &BookPackage<'_>,
    ) -> Result<(), TypesError> {
        for (name, file) in &package.content.units {
            let scope = match package.kind {
                BookKind::Mode if !file.delivers() => TypeScope::Mode,
                BookKind::Mode | BookKind::Avatar(_) | BookKind::Loadout | BookKind::Rules => {
                    TypeScope::of_package(index)
                }
            };
            types.load(scope, index, name.as_str(), &file.core())?;
        }
        if let BookKind::Avatar(unit) = package.kind {
            types.load(TypeScope::Mode, index, package.name, &unit.core())?;
        }
        Ok(())
    }

    /// The modifiers of the package at `index`, by name.
    fn modifiers(
        &mut self,
        index: PackageIndex,
        package: &BookPackage<'_>,
    ) -> Result<(), BookError> {
        let modifiers: Vec<PackageModifier<'_>> = package
            .content
            .modifiers
            .iter()
            .map(|(name, data)| {
                let script = data.script.as_ref().map(|path| self.script(index, path));
                Ok(PackageModifier {
                    name,
                    data,
                    script: script.transpose()?,
                })
            })
            .collect::<Result<_, BookError>>()?;
        let input = self.input;
        let stats = &self.stats;
        let books = &mut self.books;
        let load = ModifierLoad {
            scripts: input.scripts,
            types: &mut books.types,
            stat: |stat: &Stat| {
                stats
                    .named(stat)
                    .ok_or_else(|| UnknownName::new(NameKind::Stat, stat))
            },
            rate: input.rate,
        };
        books.modifiers.load(load, index, &modifiers).map_err(
            |ModifierError { modifier, error }| match error {
                SpecError::Problem(problem) => BookError::Modifier {
                    package: index,
                    modifier,
                    problem,
                },
                SpecError::Unknown(name) => {
                    BookError::unknown(index, BookPlace::Modifier(modifier), name)
                }
            },
        )?;
        for modifier in &modifiers {
            books.params.push_modifier(&modifier.data.params, |stat| {
                let at = BookPlace::Modifier(modifier.name.clone());
                BookBuilder::stat_in(stats, index, at, stat)
            })?;
        }
        Ok(())
    }

    /// The actions of the package at `index`, each with the ranks `ranks` gives it, 1 when it
    /// gives none, by id.
    fn actions(
        &mut self,
        index: PackageIndex,
        package: &'a BookPackage<'a>,
        ranks: impl Fn(&str) -> Option<u8>,
    ) -> Result<BTreeMap<&'a str, ActionId>, BookError> {
        package
            .content
            .actions
            .iter()
            .map(|(id, data)| {
                let ranks = ranks(id.as_str()).unwrap_or(1);
                Ok((id.as_str(), self.action(index, id, data, ranks)?))
            })
            .collect()
    }

    /// The action `id` of the package at `index`, of `ranks` ranks, with its script, its params
    /// and its effect lists; a train or a delivery waits for its unit type to bind.
    fn action(
        &mut self,
        index: PackageIndex,
        id: &DeclaredName,
        data: &'a ActionData,
        ranks: u8,
    ) -> Result<ActionId, BookError> {
        let script = data
            .script
            .as_ref()
            .map(|path| self.script(index, path))
            .transpose()?;
        let names = self.names(index, None);
        let at = || BookPlace::Action(id.clone());
        let parts = ActionParts::of(data, index, ranks, self.input.rate, &names)
            .map_err(|error| BookError::of_action(index, id, error))?;
        let action =
            self.books
                .actions
                .load(self.input.scripts, index, id.as_str(), data, script, parts);
        self.build_spec(index, id, action, data)?;
        self.requirements(index, action, data)
            .map_err(|name| BookError::unknown(index, at(), name))?;
        let stats = &self.stats;
        let run = self.books.params.push_action(&data.params, |stat| {
            BookBuilder::stat_in(stats, index, at(), stat)
        })?;
        debug_assert_eq!(run, action.index(), "one run of params per ability");
        if !(data.on_resolve.is_empty() && data.on_hit.is_empty() && data.on_end.is_empty()) {
            let mut effects = mem::take(&mut self.books.effects);
            let pushed = effects.push(action, data, &self.names(index, Some(action)));
            self.books.effects = effects;
            pushed.map_err(|error| BookError::of_action(index, id, error))?;
        }
        Ok(action)
    }

    /// The build spec of `action`, `id` of the package at `index`, when its `data` constructs: its
    /// style, its form, its life and refund at the start, and its placement rules; an error for a
    /// name the match does not have.
    fn build_spec(
        &mut self,
        index: PackageIndex,
        id: &DeclaredName,
        action: ActionId,
        data: &ActionData,
    ) -> Result<(), BookError> {
        let Some(construct) = &data.construct else {
            return Ok(());
        };
        let books = &mut self.books;
        let types = &books.types;
        let check = |rule: &PlacementRule| {
            Ok(PlacementCheck {
                filter: Filter::resolve(&rule.filter, types.tags())?,
                distance: rule.distance,
            })
        };
        let placement = data.placement.as_ref();
        let rules = |of: fn(&PlacementData) -> &[PlacementRule]| {
            placement.map_or_else(
                || Ok(Vec::new()),
                |placement| of(placement).iter().map(check).collect(),
            )
        };
        let at = |place| move |name| BookError::unknown(index, place, name);
        let style = match construct {
            ConstructData::Alone => Style::Alone,
            ConstructData::Builder => Style::Builder,
            ConstructData::Builders(rates) => Style::Builders(rates),
        };
        let input = self.input;
        let package = &input.packages[usize::from(index)];
        let building = data
            .unit_type
            .as_ref()
            .expect("a build's kind fields hold, as its parts checked");
        let place = BookPlace::UnitType(TypePlace::Declared(building.clone()));
        let file = package
            .content
            .units
            .get(building)
            .filter(|file| !file.delivers());
        let unknown = UnknownName::new(NameKind::UnitType, building);
        let file = file.ok_or_else(|| at(BookPlace::Action(id.clone()))(unknown))?;
        let problem = |problem| BookError::Action {
            package: index,
            action: id.clone(),
            problem,
        };
        let form = self
            .names
            .form(file.collision.as_ref())
            .map_err(at(place))?;
        let form = form
            .filter(|form| form.is_box())
            .ok_or_else(|| problem(ActionProblem::NoBox))?;
        let alive = input
            .data
            .combat
            .life
            .as_ref()
            .is_some_and(|life| file.pools.contains(life));
        let start_life = data.start_life;
        let starts = start_life.is_some_and(|share| !share.is_zero());
        if (alive && !starts) || start_life.is_some_and(Share::is_zero) {
            return Err(problem(ActionProblem::StartLife));
        }
        let build = NewBuild {
            form,
            style,
            start_life,
            refund: data.cancel_refund.unwrap_or(Share::ALL),
            near: rules(|placement| &placement.near).map_err(at(BookPlace::Action(id.clone())))?,
            away: rules(|placement| &placement.away).map_err(at(BookPlace::Action(id.clone())))?,
        };
        books.builds.push(action, build);
        Ok(())
    }

    /// What `action`, of the package at `index`, requires of its caster's player, when its
    /// `data` requires anything: unit types it owns and modifiers it holds; an error for a name
    /// the package does not have.
    fn requirements(
        &mut self,
        index: PackageIndex,
        action: ActionId,
        data: &ActionData,
    ) -> Result<(), UnknownName> {
        let Some(requires) = &data.requires else {
            return Ok(());
        };
        let names = self.names(index, None);
        let units: Vec<UnitType> = requires
            .units
            .iter()
            .map(|name| ActionNames::unit_type(&names, index, name))
            .collect::<Result<_, UnknownName>>()?;
        let held: Vec<ModifierId> = requires
            .modifiers
            .iter()
            .map(|name| ActionNames::modifier(&names, index, name))
            .collect::<Result<_, UnknownName>>()?;
        self.books.requirements.push(action, units, held);
        Ok(())
    }

    /// The names an action of the package at `package` resolves, with `action`'s params when it
    /// resolves effect lists.
    const fn names(&self, package: PackageIndex, action: Option<ActionId>) -> BuildNames<'_> {
        BuildNames {
            input: self.input,
            names: &self.names,
            stats: &self.stats,
            types: &self.books.types,
            modifiers: &self.books.modifiers,
            tracks: &self.books.tracks,
            params: &self.books.params.actions,
            action,
            package,
        }
    }

    /// Loads the unit types of the package at `index`, `units`, whose actions are `actions`: its
    /// delivery types, and its types that stand, each with its parts.
    fn package_units(
        &mut self,
        index: PackageIndex,
        units: &BTreeMap<DeclaredName, UnitTypeData>,
        actions: &BTreeMap<&str, ActionId>,
    ) -> Result<(), BookError> {
        for (name, file) in units {
            if file.delivers() {
                self.delivery(index, name, file)?;
            } else {
                self.unit_type(index, Standing::declared(index, name, file), actions)?;
            }
        }
        Ok(())
    }

    /// Production's parts of `unit_type`, of `file`: its queue, its supply, and what it is as a
    /// node and as a drop-off, each tagged; an error for a resource the mode does not declare.
    fn production(&mut self, unit_type: UnitType, file: &UnitTypeData) -> Result<(), UnknownName> {
        let resources = &self.names.resources;
        let resource = |name: &DeclaredName| {
            resources
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Resource, name))
        };
        let books = &mut self.books;
        if let Some(production) = file.production {
            books.producers.set(unit_type, production);
        }
        if let Some(supply) = file.supply {
            books.supplies.set(unit_type, supply);
        }
        if let Some(node) = &file.node {
            let resource = resource(&node.resource)?;
            books.types.give_tag(unit_type, Production::NODE.tag());
            books.nodes.nodes.set(unit_type, resource);
        }
        if let Some(drop_off) = &file.drop_off {
            let taken = drop_off.resources.iter().map(resource);
            let taken = taken.collect::<Result<ResourceSet, UnknownName>>()?;
            books.types.give_tag(unit_type, Production::DROP_OFF.tag());
            books.nodes.drop_offs.set(unit_type, taken);
        }
        Ok(())
    }

    /// The unit type `unit` that stands, of the package at `index`, whose slots hold the package's
    /// `actions`: its AI, its parts, its production parts, its kind of walker, and its passive.
    /// An avatar's is tagged `avatar`, and stays when it dies.
    fn unit_type(
        &mut self,
        index: PackageIndex,
        unit: Standing<'_>,
        actions: &BTreeMap<&str, ActionId>,
    ) -> Result<(), BookError> {
        let file = unit.file;
        let unit_type = self
            .books
            .types
            .types()
            .named((unit.scope, unit.name))
            .expect(DECLARED);
        let at = TypeAt {
            package: index,
            unit,
            unit_type,
        };
        let mut combat = file.combat.clone();
        if unit.declared.is_none() {
            self.books.types.give_tag(unit_type, Avatar::TAG.tag());
            if let Some(combat) = &mut combat {
                combat.on_death = OnDeath::Stay;
            }
        }
        self.ai(at)?;
        self.parts(at, combat.as_ref(), actions)?;
        let unknown = |name| at.unknown(name);
        self.production(unit_type, file).map_err(unknown)?;
        self.walker(at)?;
        let passive = self.passive(index, file).map_err(unknown)?;
        self.books
            .passives
            .set_some(unit_type, passive.map(TypePassive));
        self.books.units.unit_types.push(unit_type);
        Ok(())
    }

    /// The AI of `at`, when it takes orders.
    fn ai(&mut self, at: TypeAt<'_>) -> Result<(), BookError> {
        let Some(orders) = &at.unit.file.orders else {
            return Ok(());
        };
        let script = self.script(at.package, &orders.ai)?;
        let ai = Ai::of(orders, script, self.input.scripts, self.input.rate).map_err(|error| {
            BookError::Ai {
                package: at.package,
                unit_type: at.unit.place(),
                error,
            }
        })?;
        self.books.ais.set(at.unit_type, ai);
        Ok(())
    }

    /// What `at` starts with of its stats, with `combat`, its combat section as the type spawns
    /// with it; an error for a pool the mode does not declare, or for stats that make no unit.
    fn start(&self, at: TypeAt<'_>, combat: Option<&CombatData>) -> Result<UnitStart, BookError> {
        let data = self.input.data;
        let pools: Vec<(PoolId, &Stat)> = at
            .unit
            .file
            .pools
            .iter()
            .map(|pool| {
                let id = self.names.pools.named(pool.as_str());
                let id = id.ok_or_else(|| UnknownName::new(NameKind::Pool, pool))?;
                Ok((id, &data.pools[pool].max))
            })
            .collect::<Result<_, UnknownName>>()
            .map_err(|name| at.unknown(name))?;
        let of = StartOf {
            book: &self.stats,
            life: self.life,
            rate: self.input.rate,
            unit_type: at.unit_type,
            combat,
            pools: &pools,
        };
        UnitStart::new(of).map_err(|error| BookError::Start {
            package: at.package,
            unit_type: at.unit.place(),
            error,
        })
    }

    /// The parts `at` spawns with, each in its book: its start of its stats, with `combat`, its
    /// combat section as the type spawns with it, its sight, the body of its `collision` section,
    /// its tracks, whether it builds, what it holds as a node, whether it gathers, its inventory,
    /// its shop, its use of the package's `actions` and its use slot, and its slots of `actions`;
    /// an error for a name the mode does not declare, or for stats that make no unit.
    fn parts(
        &mut self,
        at: TypeAt<'_>,
        combat: Option<&CombatData>,
        actions: &BTreeMap<&str, ActionId>,
    ) -> Result<(), BookError> {
        let UnitStart {
            life,
            pools,
            on_death,
            step,
            turn,
        } = self.start(at, combat)?;
        let unknown = |name| at.unknown(name);
        let (file, input, unit_type) = (at.unit.file, self.input, at.unit_type);
        let body = self.names.form(file.collision.as_ref()).map_err(unknown)?;
        let names = self.names(at.package, None);
        let tracks: TrackSet = file
            .tracks
            .iter()
            .map(|track| names.track(track))
            .collect::<Result<_, UnknownName>>()
            .map_err(unknown)?;
        let inventory = file.inventory.as_ref().map(|inventory| {
            let kind = input.data.slots.names().named(inventory.kind.as_str());
            let kind = kind.ok_or_else(|| UnknownName::new(NameKind::SlotKind, &inventory.kind))?;
            Ok((Inventory::new(inventory.slots), UseGroup::new(kind)))
        });
        let inventory = inventory.transpose().map_err(unknown)?;
        let shop = file.shop.as_ref().map(|shop| self.shop(shop));
        let shop = shop.transpose().map_err(unknown)?;
        let usable = file
            .usable
            .as_ref()
            .map(|usable| self.usable(usable, actions));
        let usable = usable.transpose().map_err(unknown)?;
        let uses = file.uses.as_ref().map(|uses| {
            let kind = input.data.slots.names().named(uses.kind.as_str());
            kind.map(UseSlot::new)
                .ok_or_else(|| UnknownName::new(NameKind::SlotKind, &uses.kind))
        });
        let uses = uses.transpose().map_err(unknown)?;
        let slots = self
            .slots(file, actions, inventory.as_ref(), uses)
            .map_err(unknown)?;
        let package = &input.packages[usize::from(at.package)];
        let slotted = |kind| {
            let mut slotted = file.slots.values().flatten();
            slotted.any(|id| {
                let action = package.content.actions.get(id);
                action.is_some_and(|action| action.kind == kind)
            })
        };
        let books = &mut self.books;
        books.lives.set_some(unit_type, life);
        books.pools.set_some(unit_type, pools);
        books.deaths.set_some(unit_type, on_death);
        books.steps.set_some(unit_type, step);
        books.turns.set_some(unit_type, turn);
        books.bodies.set_some(unit_type, body);
        books
            .sights
            .set_some(unit_type, file.vision.map(|vision| vision.sight));
        books
            .type_tracks
            .set_some(unit_type, (!tracks.is_empty()).then_some(tracks));
        let builds = slotted(ActionKind::Build).then(Builder::default);
        books.builders.set_some(unit_type, builds);
        let node = file.node.as_ref().map(|node| Node::new(node.amount.get()));
        books.node_starts.set_some(unit_type, node);
        let gathers = slotted(ActionKind::Gather).then(Gatherer::default);
        books.gatherers.set_some(unit_type, gathers);
        let (inventory, group) = inventory.unzip();
        books.inventories.set_some(unit_type, inventory);
        books.use_groups.set_some(unit_type, group);
        books.shops.set_some(unit_type, shop);
        books.use_specs.set_some(unit_type, usable);
        books.use_slots.set_some(unit_type, uses);
        books.slots.set_some(unit_type, slots);
        Ok(())
    }

    /// The shop of a unit type's `shop` section, of the mode's items and resources; an error for
    /// a name the mode does not have.
    fn shop(&self, data: &ShopData) -> Result<Shop, UnknownName> {
        let items = self.books.items.names();
        let sells = data.items.iter().map(|name| {
            items
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Item, name))
        });
        let sells = sells.collect::<Result<_, UnknownName>>()?;
        let resource = self.names.resources.named(data.resource.as_str());
        let resource =
            resource.ok_or_else(|| UnknownName::new(NameKind::Resource, &data.resource))?;
        let buyers = Filter::resolve(&data.buyers, self.books.types.tags())?;
        Ok(Shop::new(
            sells,
            resource,
            data.reach,
            buyers,
            data.sell_share,
        ))
    }

    /// The use of a unit type's `use` section, its action among the package's `actions` and its
    /// users among the mode's tags; an error for a name the package or the mode does not have.
    fn usable(
        &self,
        data: &UseData,
        actions: &BTreeMap<&str, ActionId>,
    ) -> Result<UseSpec, UnknownName> {
        let action = actions.get(data.action.as_str()).copied();
        let action = action.ok_or_else(|| UnknownName::new(NameKind::Ability, &data.action))?;
        let users = Filter::resolve(&data.users, self.books.types.tags())?;
        Ok(UseSpec::new(action, users))
    }

    /// The kind of walker of `at`, of its file and its body, when it walks; an error for a box
    /// body, which only a type that stands has.
    fn walker(&mut self, at: TypeAt<'_>) -> Result<(), BookError> {
        if at.unit.file.walks() {
            let body = self.books.bodies.get(at.unit_type).copied();
            let walker = Walker::of_form(body).ok_or_else(|| BookError::BoxWalks {
                package: at.package,
                unit_type: at.unit.place(),
            })?;
            self.books.walkers.set(at.unit_type, walker);
        }
        Ok(())
    }

    /// The slots of `file`: the package's `actions` it names, kind after kind, each at its kind's
    /// first rank, then an empty slot of its `inventory`'s kind for each inventory slot, and the
    /// empty slot of its use slot's kind; none for a type with none of them, and an error for a
    /// kind the mode does not declare.
    fn slots(
        &self,
        file: &UnitTypeData,
        actions: &BTreeMap<&str, ActionId>,
        inventory: Option<&(Inventory, UseGroup)>,
        uses: Option<UseSlot>,
    ) -> Result<Option<ActionSlots>, UnknownName> {
        let slots = &self.input.data.slots;
        let mut held = Vec::new();
        for (kind, ids) in &file.slots {
            let named = slots.names().named(kind.as_str());
            let kind = named.ok_or_else(|| UnknownName::new(NameKind::SlotKind, kind))?;
            let rank = slots.first_rank(kind);
            for id in ids {
                let action = actions.get(id.as_str()).copied();
                let action = action.ok_or_else(|| UnknownName::new(NameKind::Ability, id))?;
                held.push((action, kind, rank));
            }
        }
        if held.is_empty() && inventory.is_none() && uses.is_none() {
            return Ok(None);
        }
        held.sort_by_key(|&(_, kind, _)| kind);
        let slots = ActionSlots::new(held);
        let slots = match inventory {
            Some((inventory, group)) => inventory.extend_slots(slots, *group),
            None => slots,
        };
        Ok(Some(match uses {
            Some(uses) => slots.with_empty(uses.kind(), 1),
            None => slots,
        }))
    }

    /// The passive of `file`, a modifier of the package at `index`, when it has one; an error
    /// for a modifier the package does not have.
    fn passive(
        &self,
        index: PackageIndex,
        file: &UnitTypeData,
    ) -> Result<Option<ModifierId>, UnknownName> {
        let names = self.names(index, None);
        let passive = file.passive.as_ref();
        passive
            .map(|name| ActionNames::modifier(&names, index, name))
            .transpose()
    }

    /// The projectile or area type `name` of `file`, of the package at `index`, in the scope its
    /// actions name types in, which only actions deliver, so the mode spawns none; with its sight,
    /// when it has a `vision` section.
    fn delivery(
        &mut self,
        index: PackageIndex,
        name: &DeclaredName,
        file: &UnitTypeData,
    ) -> Result<(), BookError> {
        let rate = self.input.rate;
        let books = &mut self.books;
        let scope = TypeScope::of_package(index);
        let unit_type = books
            .types
            .types()
            .named((scope, name.as_str()))
            .expect(DECLARED);
        let at = || BookPlace::UnitType(TypePlace::Declared(name.clone()));
        if let Some(projectile) = &file.projectile {
            books.types.give_tag(unit_type, Projectiles::TAG.tag());
            let spec = ProjectileSpec::of(projectile, &books.types, rate)
                .map_err(|unknown| BookError::unknown(index, at(), unknown))?;
            books.projectiles.set(unit_type, spec);
        }
        if let Some(area) = &file.area {
            books.types.give_tag(unit_type, Areas::TAG.tag());
            let modifiers = &books.modifiers;
            let modifier = |id: &DeclaredName| {
                let modifier = modifiers.names().named((index, id.as_str()));
                modifier.ok_or_else(|| UnknownName::new(NameKind::Modifier, id))
            };
            let spec =
                AreaSpec::of(area, &books.types, rate, modifier).map_err(|error| match error {
                    AreaError::Time(TimeTooLarge) => BookError::AreaTime {
                        package: index,
                        unit_type: name.clone(),
                    },
                    AreaError::Unknown(unknown) => BookError::unknown(index, at(), unknown),
                })?;
            books.areas.set(unit_type, spec);
        }
        books
            .sights
            .set_some(unit_type, file.vision.map(|vision| vision.sight));
        Ok(())
    }

    /// The ranks `types`, of the package at `index`, give each action they place; an error for
    /// an action placed in slot kinds of different ranks.
    fn slotted_ranks<'u>(
        &self,
        index: PackageIndex,
        types: impl IntoIterator<Item = &'u UnitTypeData>,
    ) -> Result<BTreeMap<&'u str, u8>, BookError> {
        let slots = types.into_iter().map(|unit_type| &unit_type.slots);
        let ranks = self.input.data.slots.slotted_ranks(slots);
        ranks.map_err(|action| BookError::Action {
            package: index,
            action: action.clone(),
            problem: ActionProblem::SlottedRanks,
        })
    }

    /// The script at `path` of the package at `index`, by its place in the order a match
    /// compiles its scripts; an error for a path the package does not hold.
    fn script(&self, index: PackageIndex, path: &PackagePath) -> Result<ScriptId, BookError> {
        let package = &self.input.packages[usize::from(index)];
        let at = package.scripts.iter().position(|held| *held == path);
        let at = at.ok_or_else(|| BookError::Script {
            package: index,
            path: path.clone(),
        })?;
        Ok(ScriptId::nth(self.script_starts[usize::from(index)] + at))
    }
}

/// The names of an action's data and effect lists as the builder resolves them: the mode's, the
/// tags of the types so far, the modifiers, and the tracks.
#[derive(Debug)]
struct BuildNames<'b> {
    input: &'b BookInput<'b>,
    names: &'b ModeNames,
    stats: &'b StatBook,
    types: &'b UnitTypeBook,
    modifiers: &'b ModifierBook,
    tracks: &'b TrackBook,
    params: &'b ParamTable,
    /// The action whose effect lists resolve, with its params.
    action: Option<ActionId>,
    package: PackageIndex,
}

impl ActionNames for BuildNames<'_> {
    fn stat(&self, stat: &Stat) -> Result<StatId, UnknownName> {
        self.stats
            .named(stat)
            .ok_or_else(|| UnknownName::new(NameKind::Stat, stat))
    }

    fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName> {
        let kind = self.names.damage_kinds.named(name.as_str());
        kind.ok_or_else(|| UnknownName::new(NameKind::DamageKind, name))
    }

    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        let names = self.names;
        CostTarget::named(&names.pools, &names.resources, name.as_str())
    }

    fn filter(&self, filter: &FilterData) -> Result<Filter, UnknownName> {
        Filter::resolve(filter, self.types.tags())
    }

    fn modifier(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<ModifierId, UnknownName> {
        let modifier = self.modifiers.names().named((package, name.as_str()));
        modifier.ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
    }

    fn unit_type(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<UnitType, UnknownName> {
        let scope = TypeScope::of_package(package);
        let unit_type = self.types.types().named((scope, name.as_str()));
        unit_type.ok_or_else(|| UnknownName::new(NameKind::UnitType, name))
    }

    fn homes(&self, package: PackageIndex, name: &DeclaredName) -> Option<bool> {
        let units = &self.input.packages[usize::from(package)].content.units;
        let projectile = units.get(name).and_then(|file| file.projectile.as_ref());
        projectile.map(|projectile| projectile.homing)
    }
}

impl EffectNames for BuildNames<'_> {
    fn param(&self, name: &DeclaredName) -> Result<usize, UnknownName> {
        let action = self.action.expect("an effect list is of an action");
        let param = self.params.named(action.index(), name.as_str());
        param.ok_or_else(|| UnknownName::new(NameKind::Param, name))
    }

    fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName> {
        ActionNames::damage_kind(self, name)
    }

    fn pool(&self, name: &DeclaredName) -> Result<PoolId, UnknownName> {
        self.names
            .pools
            .named(name.as_str())
            .ok_or_else(|| UnknownName::new(NameKind::Pool, name))
    }

    fn modifier(&self, name: &DeclaredName) -> Result<ModifierId, UnknownName> {
        ActionNames::modifier(self, self.package, name)
    }

    fn track(&self, name: &DeclaredName) -> Result<TrackId, UnknownName> {
        let track = self.tracks.names().named(name.as_str());
        track.ok_or_else(|| UnknownName::new(NameKind::Track, name))
    }

    fn tag(&self, name: &DeclaredName) -> Result<Tag, UnknownName> {
        self.types
            .tags()
            .named(name.as_str())
            .ok_or_else(|| UnknownName::new(NameKind::Tag, name))
    }

    fn package_type(&self, name: &DeclaredName) -> Result<UnitType, UnknownName> {
        self.unit_type(self.package, name)
    }
}
