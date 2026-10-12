use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;

use campfire_script::ScriptId;

use crate::actions::action_data::ActionData;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_names::ActionNames;
use crate::actions::action_parts::ActionParts;
use crate::actions::construct_data::ConstructData;
use crate::actions::cost_target::CostTarget;
use crate::actions::effect_names::EffectNames;
use crate::actions::error::ActionError;
use crate::actions::placement_data::{PlacementData, PlacementRule};
use crate::areas::Areas;
use crate::areas::area_spec::AreaSpec;
use crate::areas::error::AreaError;
use crate::books::Books;
use crate::books::book_input::{BookInput, BookKind, BookPackage};
use crate::books::book_parts::BookParts;
use crate::books::book_place::BookPlace;
use crate::books::error::BookError;
use crate::books::type_place::TypePlace;
use crate::books::unit_type_data::UnitTypeData;
use crate::combat::combat_bindings::CombatBindings;
use crate::combat::combat_data::CombatData;
use crate::combat::on_death::OnDeath;
use crate::items::item_book::{ItemBook, ItemSpec};
use crate::items::item_id::ItemId;
use crate::items::shop::{Shop, ShopPlace};
use crate::items::shop_data::ShopData;
use crate::mode::mode_books::{ModeBooks, ModeBooksInput};
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_names::ModeNames;
use crate::mode::slot_action::SlotAction;
use crate::mode::unit_kit::{InventorySpec, KitSections, UnitKit};
use crate::mode::unit_type_setup::UnitTypeSetup;
use crate::navigation::walker::Walker;
use crate::orders::ai::Ai;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::production::Production;
use crate::production::build_book::{NewBuild, PlacementCheck, Style};
use crate::production::resource_set::ResourceSet;
use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::projectiles::Projectiles;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::release::Release;
use crate::scripts::error::UnknownName;
use crate::scripts::name_kind::NameKind;
use crate::stats::error::{ModifierError, SpecError};
use crate::stats::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
use crate::stats::param_table::ParamTable;
use crate::stats::pool_book::PoolBook;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_id::StatId;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::body_form::BodyForm;
use crate::units::filter::Filter;
use crate::units::modifier_id::ModifierId;
use crate::units::tag::Tag;
use crate::units::track_id::TrackId;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::error::TimeTooLarge;
use crate::values::filter_data::FilterData;
use crate::values::names::Names;
use crate::values::package_path::PackagePath;
use crate::values::share::Share;
use crate::values::stat::Stat;

/// What the package load checked, which the builder trusts.
const CHECKED: &str = "the load checked it";

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
    fn declared(index: u16, name: &'f DeclaredName, file: &'f UnitTypeData) -> Standing<'f> {
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
        let mut books = BookParts {
            types: UnitTypes::new(Release::engine_tags()),
            supply: data.supply,
            ..BookParts::default()
        };
        for name in input.tag_names {
            books.types.declare(name.as_str());
        }
        for (index, package) in (0..).zip(&input.packages) {
            BookBuilder::declare_types(&mut books.types, index, package);
        }
        let names = ModeNames::of(data);
        let life = data.combat.life_pool(&names.pools);
        let life = life.map_err(|name| BookError::unknown(0, BookPlace::Combat, name))?;
        let stats = BookBuilder::stat_book(input, &books.types)?;
        let pools = PoolBook::new(&data.pools, Arc::clone(&names.pools), |pool, stat| {
            let at = BookPlace::Pool(pool.clone());
            BookBuilder::stat_in(&stats, 0, at, stat)
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
        package: u16,
        at: BookPlace,
        stat: &Stat,
    ) -> Result<StatId, BookError> {
        let found = book.named(stat);
        found.ok_or_else(|| BookError::unknown(package, at, UnknownName::new(NameKind::Stat, stat)))
    }

    pub(crate) fn build(mut self) -> Result<Books, BookError> {
        let input = self.input;
        if input.progression {
            self.books.tracks = Some(TrackBook::new(&input.data.tracks));
        }
        for (index, package) in (0..).zip(&input.packages) {
            self.modifiers(index, package)?;
        }
        for (index, package) in (0..).zip(&input.packages) {
            let units = &package.content.units;
            match package.kind {
                BookKind::Mode => {
                    let ranks = self.slotted_ranks(units.values());
                    let actions = self.actions(index, package, |id| ranks.get(id).copied())?;
                    self.books.items = self.item_book(index, package, &actions)?;
                    self.package_units(index, units, &actions)?;
                }
                BookKind::Avatar(unit) => {
                    let standing = units.values().filter(|file| !file.delivers());
                    let ranks = self.slotted_ranks(standing.chain([unit]));
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
        let mut parts = self.books;
        let mode_units = &input.packages[0].content.units;
        let types = &parts.types;
        let standing = |name: &str| {
            let stands = mode_units.get(name).is_some_and(|file| !file.delivers());
            stands.then(|| {
                types
                    .types()
                    .named((TypeScope::Mode, name))
                    .expect(DECLARED)
            })
        };
        let data = input.data;
        let map = ModeMap::resolve(
            input.map,
            input.teams,
            &data.relations,
            &self.names.layers,
            standing,
        )
        .map_err(BookError::Mode)?;
        let items = parts.items.names();
        let shop = data
            .shop
            .as_ref()
            .map(|shop| BookBuilder::shop(shop, items, &self.names.resources, &map))
            .transpose()
            .map_err(|name| BookError::unknown(0, BookPlace::Shop, name))?;
        let stats = &self.stats;
        let bindings = CombatBindings::new(&data.combat, |stat| {
            BookBuilder::stat_in(stats, 0, BookPlace::Combat, stat)
        })?;
        let bindings = self.life.map(|_| bindings);
        let mode = ModeBooks::build(ModeBooksInput {
            data,
            unit_types: &parts.units.unit_types,
            types: &mut parts.types,
            walkers: &parts.walkers,
            stats: self.stats,
            pools: self.pools,
            life: self.life,
            bindings,
            loadout_ranks: self.loadout_ranks,
            names: self.names,
            map,
            shop,
        });
        Ok(Books { parts, mode })
    }

    /// The item book of the mode package `package`, at `index`, its actions `actions`; an error
    /// for a name the mode does not have.
    fn item_book(
        &self,
        index: u16,
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
        Ok(ItemBook::new(ids, specs))
    }

    /// The mode's shop of `data`, selling `items` by id, for one of `resources`, at the regions of
    /// `map`'s markers with its tag, each for the marker's team; an error for a name the mode
    /// does not have.
    fn shop(
        data: &ShopData,
        items: &Names<ItemId>,
        resources: &Names<ResourceId>,
        map: &ModeMap,
    ) -> Result<Shop, UnknownName> {
        let sells = data.items.iter().map(|name| {
            items
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Item, name))
        });
        let sells = sells.collect::<Result<_, UnknownName>>()?;
        let places = map
            .markers
            .iter()
            .filter(|marker| marker.tags.contains(&data.at))
            .map(|marker| ShopPlace {
                team: marker.team.expect(CHECKED),
                region: marker.region.expect(CHECKED),
            });
        let found = resources.named(data.resource.as_str());
        let resource = found.ok_or_else(|| UnknownName::new(NameKind::Resource, &data.resource))?;
        Ok(Shop::new(
            sells,
            resource,
            places.collect(),
            data.sell_share,
        ))
    }

    /// The stat book of the mode's stats, with each unit type that stands and its `stats`
    /// section, refreshed in the input's order: each package's types that stand, in its scope,
    /// and each avatar, in the mode's; an error for a stat the mode does not declare.
    fn stat_book(input: &BookInput<'_>, types: &UnitTypes) -> Result<StatBook, BookError> {
        let mut book = StatBook::new(&input.data.stats, input.max_move_speed.get());
        for (index, package) in (0..).zip(&input.packages) {
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
    fn declare_types(types: &mut UnitTypes, index: u16, package: &BookPackage<'_>) {
        for (name, file) in &package.content.units {
            let scope = match package.kind {
                BookKind::Mode if !file.delivers() => TypeScope::Mode,
                BookKind::Mode | BookKind::Avatar(_) | BookKind::Loadout | BookKind::Rules => {
                    TypeScope::of_package(index)
                }
            };
            types.load(scope, index, name.as_str(), &file.core());
        }
        if let BookKind::Avatar(unit) = package.kind {
            types.load(TypeScope::Mode, index, package.name, &unit.core());
        }
    }

    /// The modifiers of the package at `index`, by name.
    fn modifiers(&mut self, index: u16, package: &BookPackage<'_>) -> Result<(), BookError> {
        let modifiers: Vec<PackageModifier<'_>> = package
            .content
            .modifiers
            .iter()
            .map(|(name, data)| PackageModifier {
                name,
                data,
                script: data.script.as_ref().map(|path| self.script(index, path)),
            })
            .collect();
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
        index: u16,
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
        index: u16,
        id: &DeclaredName,
        data: &'a ActionData,
        ranks: u8,
    ) -> Result<ActionId, BookError> {
        let script = data.script.as_ref().map(|path| self.script(index, path));
        let names = self.names(index, None);
        let at = || BookPlace::Action(id.clone());
        let parts =
            ActionParts::of(data, index, ranks, self.input.rate, &names).map_err(|error| {
                match error {
                    ActionError::Time(error) => BookError::Action {
                        package: index,
                        action: id.clone(),
                        error,
                    },
                    ActionError::Unknown(name) => BookError::unknown(index, at(), name),
                }
            })?;
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
            pushed.map_err(|name| BookError::unknown(index, at(), name))?;
        }
        Ok(action)
    }

    /// The build spec of `action`, `id` of the package at `index`, when its `data` constructs: its
    /// style, its form, its life and refund at the start, and its placement rules; an error for a
    /// name the match does not have.
    fn build_spec(
        &mut self,
        index: u16,
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
                filter: Filter::resolve(&rule.filter, types)?,
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
        let package = &self.input.packages[usize::from(index)];
        let building = data.unit_type.as_ref().expect(CHECKED);
        let collision = package.content.units[building].collision.as_ref();
        let place = BookPlace::UnitType(TypePlace::Declared(building.clone()));
        let build = NewBuild {
            form: self
                .names
                .form(collision)
                .map_err(at(place))?
                .expect(CHECKED),
            style,
            start_life: data.start_life,
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
        index: u16,
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
    fn names(&self, package: u16, action: Option<ActionId>) -> BuildNames<'_> {
        BuildNames {
            input: self.input,
            names: &self.names,
            stats: &self.stats,
            types: &self.books.types,
            modifiers: &self.books.modifiers,
            tracks: self.books.tracks.as_ref(),
            params: &self.books.params.actions,
            action,
            package,
        }
    }

    /// Loads the unit types of the package at `index`, `units`, whose actions are `actions`: its
    /// delivery types, and its types that stand, each with its kit.
    fn package_units(
        &mut self,
        index: u16,
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
    /// `actions`: its AI, its kit of the values `stats` gives it, its production parts, its kind
    /// of walker, its slots, kind after kind, and its passive. An avatar's is tagged `avatar`,
    /// and stays when it dies.
    fn unit_type(
        &mut self,
        index: u16,
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
        let unknown = |name| BookError::unknown(index, unit.at(), name);
        let body = self.names.form(file.collision.as_ref()).map_err(unknown)?;
        let mut combat = file.combat.clone();
        if unit.declared.is_none() {
            self.books.types.give_tag(unit_type, Units::AVATAR.tag());
            if let Some(combat) = &mut combat {
                combat.on_death = OnDeath::Stay;
            }
        }
        self.ai(index, unit, unit_type)?;
        let kit = self.kit(index, unit, unit_type, combat.as_ref(), body)?;
        self.production(unit_type, file).map_err(unknown)?;
        self.walker(unit_type, file, body);
        let setup = UnitTypeSetup {
            unit_type,
            kit,
            actions: self.slot_actions(file, actions).map_err(unknown)?,
            passive: self.passive(index, file).map_err(unknown)?,
        };
        self.books.units.unit_types.push(setup);
        Ok(())
    }

    /// The AI of `unit`, of the package at `index`, `unit_type`, when it takes orders.
    fn ai(&mut self, index: u16, unit: Standing<'_>, unit_type: UnitType) -> Result<(), BookError> {
        let Some(orders) = &unit.file.orders else {
            return Ok(());
        };
        let script = self.script(index, &orders.ai);
        let ai = Ai::of(orders, script, self.input.scripts, self.input.rate).map_err(|error| {
            BookError::Ai {
                package: index,
                unit_type: unit.place(),
                error,
            }
        })?;
        self.books.ais.set(unit_type, ai);
        Ok(())
    }

    /// The kit of `unit`, of the package at `index`, `unit_type`, with `combat`, its combat
    /// section as the type spawns with it, and `body`, the body of its `collision` section; an
    /// error for a name the mode does not declare.
    fn kit(
        &self,
        index: u16,
        unit: Standing<'_>,
        unit_type: UnitType,
        combat: Option<&CombatData>,
        body: Option<BodyForm>,
    ) -> Result<UnitKit, BookError> {
        let (file, data) = (unit.file, self.input.data);
        let unknown = |name| BookError::unknown(index, unit.at(), name);
        let pools: Vec<(PoolId, &Stat)> = file
            .pools
            .iter()
            .map(|pool| {
                let id = self.names.pools.named(pool.as_str());
                let id = id.ok_or_else(|| UnknownName::new(NameKind::Pool, pool))?;
                Ok((id, &data.pools[pool].max))
            })
            .collect::<Result<_, UnknownName>>()
            .map_err(unknown)?;
        let names = self.names(index, None);
        let tracks: TrackSet = file
            .tracks
            .iter()
            .map(|track| names.track(track))
            .collect::<Result<_, UnknownName>>()
            .map_err(unknown)?;
        let inventory = file.inventory.as_ref().map(|inventory| {
            let kind = data.slots.names().named(inventory.kind.as_str());
            Ok(InventorySpec {
                slots: inventory.slots,
                kind: kind.ok_or_else(|| UnknownName::new(NameKind::SlotKind, &inventory.kind))?,
            })
        });
        let inventory = inventory.transpose().map_err(unknown)?;
        let package = &self.input.packages[usize::from(index)];
        let slotted = |kind| {
            let mut slotted = file.slots.values().flatten();
            slotted.any(|id| package.content.actions[id].kind == kind)
        };
        let sections = KitSections {
            combat,
            pools,
            vision: file.vision.as_ref(),
            body,
            tracks,
            production: file.production.as_ref(),
            builds: slotted(ActionKind::Build),
            node: file.node.as_ref().map(|node| node.amount),
            gathers: slotted(ActionKind::Gather),
            inventory,
        };
        let rate = self.input.rate;
        UnitKit::new(&self.stats, unit_type, sections, self.life, rate).map_err(|error| {
            BookError::Kit {
                package: index,
                unit_type: unit.place(),
                error,
            }
        })
    }

    /// The kind of walker of `unit_type`, of `file` and of `body`, the body of its `collision`
    /// section, when it walks.
    fn walker(&mut self, unit_type: UnitType, file: &UnitTypeData, body: Option<BodyForm>) {
        if file.walks() {
            let walker = Walker::of_form(body).expect("a unit type that walks has no box");
            self.books.walkers.set(unit_type, walker);
        }
    }

    /// The actions in the slots of `file`, of the package's `actions`, kind after kind, each at
    /// its kind's first rank; an error for a kind the mode does not declare.
    fn slot_actions(
        &self,
        file: &UnitTypeData,
        actions: &BTreeMap<&str, ActionId>,
    ) -> Result<Vec<SlotAction>, UnknownName> {
        let slots = &self.input.data.slots;
        let mut held = Vec::new();
        for (kind, ids) in &file.slots {
            let named = slots.names().named(kind.as_str());
            let kind = named.ok_or_else(|| UnknownName::new(NameKind::SlotKind, kind))?;
            let rank = slots.first_rank(kind);
            held.extend(ids.iter().map(|id| SlotAction {
                kind,
                action: actions[id.as_str()],
                rank,
            }));
        }
        held.sort_by_key(|action| action.kind);
        Ok(held)
    }

    /// The passive of `file`, a modifier of the package at `index`, when it has one; an error
    /// for a modifier the package does not have.
    fn passive(&self, index: u16, file: &UnitTypeData) -> Result<Option<ModifierId>, UnknownName> {
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
        index: u16,
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
        if let Some(vision) = file.vision {
            books.sights.set(unit_type, vision.sight);
        }
        Ok(())
    }

    /// The ranks `types` give each action they place, which the load checked agree.
    fn slotted_ranks<'u>(
        &self,
        types: impl IntoIterator<Item = &'u UnitTypeData>,
    ) -> BTreeMap<&'u str, u8> {
        let slots = types.into_iter().map(|unit_type| &unit_type.slots);
        self.input.data.slots.slotted_ranks(slots).expect(CHECKED)
    }

    /// The script at `path` of the package at `index`, by its place in the order a match
    /// compiles its scripts.
    fn script(&self, index: u16, path: &PackagePath) -> ScriptId {
        let package = &self.input.packages[usize::from(index)];
        let at = package
            .scripts
            .iter()
            .position(|held| *held == path)
            .expect(CHECKED);
        ScriptId::nth(self.script_starts[usize::from(index)] + at)
    }
}

/// The names of an action's data and effect lists as the builder resolves them: the mode's, the
/// tags of the types so far, the modifiers, and the tracks.
#[derive(Debug)]
struct BuildNames<'b> {
    input: &'b BookInput<'b>,
    names: &'b ModeNames,
    stats: &'b StatBook,
    types: &'b UnitTypes,
    modifiers: &'b ModifierBook,
    tracks: Option<&'b TrackBook>,
    params: &'b ParamTable,
    /// The action whose effect lists resolve, with its params.
    action: Option<ActionId>,
    package: u16,
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
        Filter::resolve(filter, self.types)
    }

    fn modifier(&self, package: u16, name: &DeclaredName) -> Result<ModifierId, UnknownName> {
        let modifier = self.modifiers.names().named((package, name.as_str()));
        modifier.ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
    }

    fn unit_type(&self, package: u16, name: &DeclaredName) -> Result<UnitType, UnknownName> {
        let scope = TypeScope::of_package(package);
        let unit_type = self.types.types().named((scope, name.as_str()));
        unit_type.ok_or_else(|| UnknownName::new(NameKind::UnitType, name))
    }

    fn homes(&self, package: u16, name: &DeclaredName) -> bool {
        let units = &self.input.packages[usize::from(package)].content.units;
        let projectile = units.get(name).and_then(|file| file.projectile.as_ref());
        projectile.expect(CHECKED).homing
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
        let track = self
            .tracks
            .and_then(|tracks| tracks.names().named(name.as_str()));
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
