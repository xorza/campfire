use std::collections::BTreeMap;

use campfire_script::ScriptId;

use crate::actions::action_data::ActionData;
use crate::actions::action_names::ActionNames;
use crate::actions::action_parts::ActionParts;
use crate::actions::cost_target::CostTarget;
use crate::actions::effect_names::EffectNames;
use crate::areas::area_spec::AreaSpec;
use crate::books::book_input::{BookInput, BookKind, BookPackage};
use crate::books::error::BookError;
use crate::books::type_place::TypePlace;
use crate::books::unit_type_file::UnitTypeFile;
use crate::books::{BookParts, Books};
use crate::combat::on_death::OnDeath;
use crate::items::item_book::{ItemBook, ItemSpec};
use crate::items::item_data::ItemData;
use crate::items::item_id::ItemId;
use crate::items::shop::{Shop, ShopPlace};
use crate::items::shop_data::ShopData;
use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_setup::{SlotAction, UnitTypeSetup};
use crate::mode::unit_kit::{InventorySpec, KitSections, UnitKit};
use crate::orders::ai::Ai;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::stats::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
use crate::stats::param_table::ParamTable;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_id::StatId;
use crate::units::action_id::ActionId;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::modifier_id::ModifierId;
use crate::units::tag::Tag;
use crate::units::track_id::TrackId;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::package_path::PackagePath;
use crate::values::stat::Stat;

/// What the package load checked, which the builder trusts.
const CHECKED: &str = "the load checked it";

/// Builds a match's books from its packages, in the order a match loads them: the tags, the
/// tracks and every package's modifiers first; then every package's unit types, by name; then
/// each package's actions and the unit types' parts, the mode's, then each dependency's.
#[derive(Debug)]
pub(crate) struct BookBuilder<'a> {
    input: &'a BookInput<'a>,
    books: BookParts,
    /// The mode's stats, for every unit type, which every stat the packages name resolves by.
    stats: StatBook,
    /// The life pool, none when the mode names none.
    life: Option<PoolId>,
    /// Where each package's scripts start among the match's.
    script_starts: Vec<usize>,
}

/// A unit type that stands, as its package gives it: its name in the mode's scope, its file, and
/// the name its package declares it by, none for the avatar its package stands as.
#[derive(Debug, Clone, Copy)]
struct Standing<'f> {
    name: &'f str,
    file: &'f UnitTypeFile,
    declared: Option<&'f DeclaredName>,
}

impl<'f> Standing<'f> {
    fn declared(name: &'f DeclaredName, file: &'f UnitTypeFile) -> Standing<'f> {
        Standing {
            name: name.as_str(),
            file,
            declared: Some(name),
        }
    }

    /// The avatar of the package `package` names.
    const fn avatar(package: &'f str, file: &'f UnitTypeFile) -> Standing<'f> {
        Standing {
            name: package,
            file,
            declared: None,
        }
    }

    fn place(&self) -> TypePlace {
        self.declared
            .map_or(TypePlace::Avatar, |name| TypePlace::Declared(name.clone()))
    }
}

impl<'a> BookBuilder<'a> {
    /// The builder of `input`'s books, its tags and unit types declared, in the order their ids
    /// follow, and its stat book built for them.
    pub(crate) fn new(input: &'a BookInput<'a>) -> BookBuilder<'a> {
        let data = input.data;
        let mut script_starts = Vec::with_capacity(input.packages.len());
        let mut start = 0;
        for package in &input.packages {
            script_starts.push(start);
            start += package.scripts.len();
        }
        let mut books = BookParts::default();
        for name in &input.tag_names {
            books.types.declare(name);
        }
        for (index, package) in (0..).zip(&input.packages) {
            BookBuilder::declare_types(&mut books.types, index, package);
        }
        BookBuilder {
            input,
            stats: BookBuilder::stat_book(input, &books.types),
            books,
            life: data.combat.life_pool(&data.pools),
            script_starts,
        }
    }

    pub(crate) fn build(mut self) -> Result<Books, BookError> {
        let input = self.input;
        if input.progression {
            self.books.tracks = Some(TrackBook::new(&input.data.tracks));
        }
        for (index, package) in (0..).zip(&input.packages) {
            self.modifiers(index, package)?;
        }
        let loadout_ranks = input.data.loadout_ranks();
        for (index, package) in (0..).zip(&input.packages) {
            let units = &package.content.units;
            match package.kind {
                BookKind::Mode => {
                    let ranks = self.slotted_ranks(units.values());
                    let actions = self.actions(index, package, |id| ranks.get(id).copied())?;
                    self.books.items = self.item_book(index, package, &actions);
                    for (name, file) in units {
                        if file.delivers() {
                            self.delivery(index, name, file)?;
                        } else {
                            let unit = Standing::declared(name, file);
                            self.unit_type(index, unit, &actions)?;
                        }
                    }
                }
                BookKind::Avatar(unit) => {
                    for (name, file) in units {
                        self.delivery(index, name, file)?;
                    }
                    let ranks = self.slotted_ranks([unit]);
                    let actions = self.actions(index, package, |id| ranks.get(id).copied())?;
                    let unit = Standing::avatar(package.name, unit);
                    self.unit_type(index, unit, &actions)?;
                    self.books.units.avatars.push(package.name);
                }
                BookKind::Loadout => {
                    for (name, file) in units {
                        self.delivery(index, name, file)?;
                    }
                    let actions = self.actions(index, package, |_| Some(loadout_ranks))?;
                    for (id, ability) in actions {
                        self.books.units.loadout.push(id, ability);
                    }
                }
            }
        }
        let mut parts = self.books;
        let stats = self.stats;
        let mode_units = &input.packages[0].content.units;
        let types = &parts.types;
        let standing = |name: &str| {
            let stands = mode_units.get(name).is_some_and(|file| !file.delivers());
            stands.then(|| types.named(TypeScope::Mode, name).expect(CHECKED))
        };
        let data = input.data;
        let map = ModeMap::resolve(
            input.map,
            input.teams,
            &data.relations,
            &data.navigation,
            standing,
        )
        .map_err(BookError::Mode)?;
        let items = &input.packages[0].content.items;
        let shop = data
            .shop
            .as_ref()
            .map(|shop| BookBuilder::shop(shop, items, &data.resources, &map));
        let mode = ModeBooks::build(
            input.data,
            &parts.units.unit_types,
            &mut parts.types,
            stats,
            map,
            shop,
        );
        Ok(Books { parts, mode })
    }

    /// The item book of the mode package `package`, at `index`, its actions `actions`.
    fn item_book(
        &self,
        index: u16,
        package: &BookPackage<'_>,
        actions: &BTreeMap<&str, ActionId>,
    ) -> ItemBook {
        let items = &package.content.items;
        let id = |name: &DeclaredName| {
            let at = items.keys().position(|other| other == name).expect(CHECKED);
            ItemId::nth(u32::try_from(at).expect("a mode's item count fits u32"))
        };
        let resources = &self.input.data.resources;
        let specs = items.values().map(|item| ItemSpec {
            cost: item
                .cost
                .iter()
                .map(|(resource, &amount)| ResourceAmount {
                    resource: ResourceId::named(resources, resource.as_str()).expect(CHECKED),
                    amount: i64::from(amount),
                })
                .collect(),
            components: item.components.iter().map(id).collect(),
            stack: item.stack,
            uses: item.uses,
            modifiers: item
                .modifiers
                .iter()
                .map(|modifier| {
                    let modifiers = &self.books.modifiers;
                    modifiers.named(index, modifier.as_str()).expect(CHECKED)
                })
                .collect(),
            action: item.action.as_ref().map(|action| actions[action.as_str()]),
        });
        ItemBook::new(specs.collect())
    }

    /// The mode's shop of `data`, selling `items` by id, for one of `resources`, at the regions of
    /// `map`'s markers with its tag, each for the marker's team.
    fn shop(
        data: &ShopData,
        items: &BTreeMap<DeclaredName, ItemData>,
        resources: &[DeclaredName],
        map: &ModeMap,
    ) -> Shop {
        let sells = data.items.iter().map(|name| {
            let at = items.keys().position(|other| other == name).expect(CHECKED);
            ItemId::nth(u32::try_from(at).expect("a mode's item count fits u32"))
        });
        let places = map
            .markers
            .iter()
            .filter(|marker| marker.tags.named(data.at.as_str()).is_some())
            .map(|marker| ShopPlace {
                team: marker.team.expect(CHECKED),
                region: marker.region.expect(CHECKED),
            });
        let resource = ResourceId::named(resources, data.resource.as_str()).expect(CHECKED);
        Shop::new(sells.collect(), resource, places.collect(), data.sell_share)
    }

    /// The stat book of the mode's stats, with each unit type that stands and its `stats`
    /// section, refreshed in the input's order.
    fn stat_book(input: &BookInput<'_>, types: &UnitTypes) -> StatBook {
        let standing = input.packages.iter().flat_map(|package| {
            let units = package.content.units.iter();
            let mode = units
                .filter(|(_, file)| matches!(package.kind, BookKind::Mode) && !file.delivers())
                .map(|(name, file)| (name.as_str(), file));
            let avatar = match package.kind {
                BookKind::Avatar(unit) => Some((package.name, unit)),
                BookKind::Mode | BookKind::Loadout => None,
            };
            mode.chain(avatar)
        });
        let setups = standing.filter_map(|(name, file)| {
            let unit_type = types.named(TypeScope::Mode, name).expect(CHECKED);
            Some((unit_type, file.stats.as_ref()?))
        });
        let max_move_speed = input.max_move_speed.get();
        StatBook::new(&input.data.stats, setups, max_move_speed)
            .with_order(input.stat_order.clone())
    }

    /// Declares the unit types of the package at `index` in the order its own load reads them,
    /// which numbers them: the mode's by name, each a delivery type or one that stands; a
    /// package's delivery types by name, then its avatar.
    fn declare_types(types: &mut UnitTypes, index: u16, package: &BookPackage<'_>) {
        for (name, file) in &package.content.units {
            let scope = match package.kind {
                BookKind::Mode if !file.delivers() => TypeScope::Mode,
                BookKind::Mode | BookKind::Avatar(_) | BookKind::Loadout => {
                    TypeScope::of_package(index)
                }
            };
            types.load(scope, name.as_str(), &file.core);
        }
        if let BookKind::Avatar(unit) = package.kind {
            types.load(TypeScope::Mode, package.name, &unit.core);
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
            stat: |stat: &Stat| stats.named(stat).expect(CHECKED),
            rate: input.rate,
        };
        books
            .modifiers
            .load(load, index, &modifiers)
            .map_err(|error| BookError::Modifier {
                package: index,
                modifier: error.modifier,
                problem: error.problem,
            })?;
        for modifier in &modifiers {
            books.params.push_modifier(&modifier.data.params, |stat| {
                stats.named(stat).expect(CHECKED)
            });
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
        let names = BuildNames {
            input: self.input,
            stats: &self.stats,
            types: &self.books.types,
            modifiers: &self.books.modifiers,
            tracks: self.books.tracks.as_ref(),
            params: &self.books.params.actions,
            action: None,
            package: index,
        };
        let parts =
            ActionParts::of(data, index, ranks, self.input.rate, &names).map_err(|error| {
                BookError::Action {
                    package: index,
                    action: id.clone(),
                    error,
                }
            })?;
        let books = &mut self.books;
        let action =
            books
                .actions
                .load(self.input.scripts, index, id.as_str(), data, script, parts);
        let run = books
            .params
            .push_action(&data.params, |stat| self.stats.named(stat).expect(CHECKED));
        debug_assert_eq!(run, action.index(), "one run of params per ability");
        if !(data.on_resolve.is_empty() && data.on_hit.is_empty() && data.on_end.is_empty()) {
            let names = BuildNames {
                input: self.input,
                stats: &self.stats,
                types: &books.types,
                modifiers: &books.modifiers,
                tracks: books.tracks.as_ref(),
                params: &books.params.actions,
                action: Some(action),
                package: index,
            };
            books.effects.push(action, data, &names);
        }
        Ok(action)
    }

    /// The unit type `unit` that stands, of the package at `index`, whose slots hold the package's
    /// `actions`: its AI, its kit of the values `stats` gives it, its slots, kind after kind, and
    /// its passive. An avatar's is tagged `avatar`, and stays when it dies.
    fn unit_type(
        &mut self,
        index: u16,
        unit: Standing<'_>,
        actions: &BTreeMap<&str, ActionId>,
    ) -> Result<(), BookError> {
        let Standing { name, file, .. } = unit;
        let avatar = unit.declared.is_none();
        let data = self.input.data;
        let books = &mut self.books;
        let unit_type = books.types.named(TypeScope::Mode, name).expect(CHECKED);
        let mut combat = file.combat.clone();
        if avatar {
            books.types.give_tag(unit_type, EngineTag::Avatar.tag());
            if let Some(combat) = &mut combat {
                combat.on_death = OnDeath::Stay;
            }
        }
        if let Some(orders) = &file.orders {
            let script = self.script(index, &orders.ai);
            let books = &mut self.books;
            let ai =
                Ai::of(orders, script, self.input.scripts, self.input.rate).map_err(|error| {
                    BookError::Ai {
                        package: index,
                        unit_type: unit.place(),
                        error,
                    }
                })?;
            books.ais.set(unit_type, ai);
        }
        let books = &mut self.books;
        let pools = file.pools.iter().map(|pool| {
            let id = PoolId::named(&data.pools, pool).expect(CHECKED);
            (id, &data.pools[pool].max)
        });
        let tracks = TrackSet::of(file.tracks.iter().map(|track| {
            let tracks = books.tracks.as_ref().expect(CHECKED);
            tracks.named(track.as_str()).expect(CHECKED)
        }));
        let rate = self.input.rate;
        let sections = KitSections {
            combat: combat.as_ref(),
            pools,
            vision: file.vision.as_ref(),
            body: data.navigation.body(file.collision.as_ref()),
            tracks,
            production: file.production.as_ref(),
            inventory: file.inventory.as_ref().map(|inventory| InventorySpec {
                slots: inventory.slots,
                kind: data.slots.named(inventory.kind.as_str()).expect(CHECKED),
            }),
        };
        let kit =
            UnitKit::new(&self.stats, unit_type, sections, self.life, rate).map_err(|error| {
                BookError::Kit {
                    package: index,
                    unit_type: unit.place(),
                    error,
                }
            })?;
        if let Some(production) = file.production {
            books.producers.set(unit_type, production);
        }
        let mut slots = Vec::new();
        for (kind, ids) in &file.slots {
            let kind = data.slots.named(kind.as_str()).expect(CHECKED);
            let rank = data.slots.first_rank(kind);
            let slotted = ids.iter().map(|id| SlotAction {
                kind,
                ability: actions[id.as_str()],
                rank,
            });
            slots.extend(slotted);
        }
        slots.sort_by_key(|action| action.kind);
        let passive = file.passive.as_ref().map(|passive| {
            books
                .modifiers
                .named(index, passive.as_str())
                .expect(CHECKED)
        });
        books.units.unit_types.push(UnitTypeSetup {
            unit_type,
            kit,
            actions: slots,
            passive,
        });
        Ok(())
    }

    /// The projectile or area type `name` of `file`, of the package at `index`, in the scope its
    /// actions name types in, which only actions deliver, so the mode spawns none; with its sight,
    /// when it has a `vision` section.
    fn delivery(
        &mut self,
        index: u16,
        name: &DeclaredName,
        file: &UnitTypeFile,
    ) -> Result<(), BookError> {
        let rate = self.input.rate;
        let books = &mut self.books;
        let scope = TypeScope::of_package(index);
        let unit_type = books.types.named(scope, name.as_str()).expect(CHECKED);
        if let Some(projectile) = &file.projectile {
            books.types.give_tag(unit_type, EngineTag::Projectile.tag());
            let spec = ProjectileSpec::of(projectile, &books.types, rate);
            books.projectiles.set(unit_type, spec);
        }
        if let Some(area) = &file.area {
            books.types.give_tag(unit_type, EngineTag::Area.tag());
            let modifiers = &books.modifiers;
            let modifier = |id: &DeclaredName| modifiers.named(index, id.as_str()).expect(CHECKED);
            let spec = AreaSpec::of(area, &books.types, rate, modifier).ok_or_else(|| {
                BookError::AreaTime {
                    package: index,
                    unit_type: name.clone(),
                }
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
        types: impl IntoIterator<Item = &'u UnitTypeFile>,
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
    fn stat(&self, stat: &Stat) -> StatId {
        self.stats.named(stat).expect(CHECKED)
    }

    fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
        self.input.damage_kind(name)
    }

    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        self.input.data.cost_target_named(name)
    }

    fn filter(&self, filter: &FilterData) -> Filter {
        Filter::resolve(filter, self.types).expect(CHECKED)
    }

    fn modifier(&self, package: u16, name: &DeclaredName) -> ModifierId {
        self.modifiers.named(package, name.as_str()).expect(CHECKED)
    }

    fn unit_type(&self, package: u16, name: &DeclaredName) -> UnitType {
        let scope = TypeScope::of_package(package);
        self.types.named(scope, name.as_str()).expect(CHECKED)
    }

    fn homes(&self, package: u16, name: &DeclaredName) -> bool {
        let units = &self.input.packages[usize::from(package)].content.units;
        let projectile = units.get(name).and_then(|file| file.projectile.as_ref());
        projectile.expect(CHECKED).homing
    }
}

impl EffectNames for BuildNames<'_> {
    fn param(&self, name: &DeclaredName) -> usize {
        let action = self.action.expect("an effect list is of an action");
        self.params
            .named(action.index(), name.as_str())
            .expect(CHECKED)
    }

    fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
        self.input.damage_kind(name)
    }

    fn pool(&self, name: &DeclaredName) -> PoolId {
        PoolId::named(&self.input.data.pools, name).expect(CHECKED)
    }

    fn modifier(&self, name: &DeclaredName) -> ModifierId {
        self.modifiers
            .named(self.package, name.as_str())
            .expect(CHECKED)
    }

    fn track(&self, name: &DeclaredName) -> TrackId {
        self.tracks
            .and_then(|tracks| tracks.named(name.as_str()))
            .expect(CHECKED)
    }

    fn tag(&self, name: &DeclaredName) -> Tag {
        self.types.tag_named(name.as_str()).expect(CHECKED)
    }

    fn delivery_type(&self, name: &DeclaredName) -> UnitType {
        self.unit_type(self.package, name)
    }

    fn standing_type(&self, name: &DeclaredName) -> UnitType {
        self.types
            .named(TypeScope::Mode, name.as_str())
            .expect(CHECKED)
    }
}
