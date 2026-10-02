use std::collections::BTreeMap;

use campfire_content::PackagePath;
use campfire_script::ScriptId;

use crate::abilities::effect_lists::Listed;
use crate::abilities::effect_names::EffectNames;
use crate::actions::action_book::{ActionId, ActionParts};
use crate::actions::action_data::{ActionData, CostTarget};
use crate::actions::action_names::ActionNames;
use crate::areas::area_spec::AreaSpec;
use crate::books::book_input::{BookInput, BookKind, BookPackage};
use crate::books::error::BookError;
use crate::books::unit_type_file::UnitTypeFile;
use crate::books::{AbilityName, Books, Spawn};
use crate::combat::damage_kind::DamageKind;
use crate::combat::on_death::OnDeath;
use crate::mode::mode_setup::{LoadoutSetup, SlotAction, UnitTypeSetup};
use crate::mode::unit_kit::{KitRules, UnitKit};
use crate::orders::ai::Ai;
use crate::progression::track_book::TrackBook;
use crate::progression::track_id::TrackId;
use crate::progression::track_set::TrackSet;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::stats::modifier_book::{ModifierBook, ModifierId};
use crate::stats::param_table::ParamTable;
use crate::stats::pool_id::PoolId;
use crate::stats::stat::Stat;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::type_scope::TypeScope;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;

/// What the package load checked, which the builder trusts.
const CHECKED: &str = "the load checked it";

/// Builds a match's books from its packages, in the order a match loads them: the tags, the
/// tracks and every package's modifiers first; then each package's actions and unit types, the
/// mode's, then each dependency's; then the unit type each train and delivery spawns.
#[derive(Debug)]
pub(crate) struct BookBuilder<'a> {
    input: &'a BookInput<'a>,
    books: Books,
    rules: KitRules,
    /// Where each package's scripts start among the match's.
    script_starts: Vec<usize>,
    /// Each train and delivery, its package, and the name of the unit type it spawns there.
    spawns: Vec<(ActionId, u16, &'a DeclaredName)>,
}

impl<'a> BookBuilder<'a> {
    pub(crate) fn new(input: &'a BookInput<'a>) -> BookBuilder<'a> {
        let data = input.data;
        let mut script_starts = Vec::with_capacity(input.packages.len());
        let mut start = 0;
        for package in &input.packages {
            script_starts.push(start);
            start += package.scripts.len();
        }
        BookBuilder {
            input,
            books: Books::default(),
            rules: KitRules {
                rate: input.rate,
                max_move_speed: input.max_move_speed,
                life: data.combat.life_pool(&data.pools).unwrap_or(PoolId::FIRST),
            },
            script_starts,
            spawns: Vec::new(),
        }
    }

    pub(crate) fn build(mut self) -> Result<Books, BookError> {
        let input = self.input;
        for name in &input.tag_names {
            self.books.types.declare(name).expect(CHECKED);
        }
        if input.progression {
            self.books.tracks = Some(TrackBook::new(&input.data.tracks));
        }
        for (index, package) in (0..).zip(&input.packages) {
            self.modifiers(index, package);
        }
        let loadout_ranks = input.data.loadout_ranks();
        for (index, package) in (0..).zip(&input.packages) {
            let units = &package.content.units;
            match package.kind {
                BookKind::Mode => {
                    let ranks = self.slotted_ranks(units.values());
                    let actions = self.actions(index, package, |id| ranks.get(id).copied())?;
                    for (name, file) in units {
                        if file.delivers() {
                            self.delivery(index, name, file)?;
                        } else {
                            self.unit_type(index, name.as_str(), file, &actions, false)?;
                        }
                    }
                }
                BookKind::Avatar(unit) => {
                    for (name, file) in units {
                        self.delivery(index, name, file)?;
                    }
                    let ranks = self.slotted_ranks([unit]);
                    let actions = self.actions(index, package, |id| ranks.get(id).copied())?;
                    self.unit_type(index, package.name, unit, &actions, true)?;
                    self.books.units.avatars.push(package.name.to_owned());
                }
                BookKind::Loadout => {
                    for (name, file) in units {
                        self.delivery(index, name, file)?;
                    }
                    let actions = self.actions(index, package, |_| Some(loadout_ranks))?;
                    let entries = actions.into_iter().map(|(id, ability)| LoadoutSetup {
                        id: id.to_owned(),
                        ability,
                    });
                    self.books.units.loadout.extend(entries);
                }
            }
        }
        for &(action, package, name) in &self.spawns {
            let scope = TypeScope::of_package(package);
            let unit_type = self.books.types.named(scope, name.as_str()).expect(CHECKED);
            self.books.actions.bind_spawn(action, unit_type);
            self.books.spawns.push(Spawn { action, unit_type });
        }
        Ok(self.books)
    }

    /// The modifiers of the package at `index`, by name.
    fn modifiers(&mut self, index: u16, package: &BookPackage<'_>) {
        for (name, data) in &package.content.modifiers {
            let script = data.script.as_ref().map(|path| self.script(index, path));
            let books = &mut self.books;
            let id = books.modifiers.load(
                self.input.scripts,
                &mut books.types,
                index,
                name.as_str(),
                data,
                script,
            );
            let run = books
                .modifier_params
                .push(&data.params, |stat| self.input.stat(stat));
            debug_assert_eq!(run, id.index(), "one run of params per modifier");
        }
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
            types: &self.books.types,
            modifiers: &self.books.modifiers,
            tracks: self.books.tracks.as_ref(),
            params: &self.books.action_params,
            action: None,
            package: index,
        };
        let parts =
            ActionParts::of(data, index, ranks, self.input.rate, &names).map_err(|error| {
                BookError::Action {
                    package: index,
                    action: id.to_string(),
                    error,
                }
            })?;
        let books = &mut self.books;
        let action = books
            .actions
            .load(self.input.scripts, index, data, script, parts);
        let run = books
            .action_params
            .push(&data.params, |stat| self.input.stat(stat));
        debug_assert_eq!(run, action.index(), "one run of params per ability");
        let delivery = books.actions.get(action).and_then(|held| held.delivery);
        books.abilities.push(AbilityName {
            name: id.as_str().into(),
            delivery,
        });
        if !(data.on_resolve.is_empty() && data.on_hit.is_empty() && data.on_end.is_empty()) {
            let names = BuildNames {
                input: self.input,
                types: &books.types,
                modifiers: &books.modifiers,
                tracks: books.tracks.as_ref(),
                params: &books.action_params,
                action: Some(action),
                package: index,
            };
            let lists = Listed::lists_of(data, &names);
            books.effects.push(action, lists);
        }
        if let Some(unit_type) = &data.unit_type {
            self.spawns.push((action, index, unit_type));
        }
        if let Some(delivery) = &data.delivery {
            self.spawns.push((action, index, delivery.unit_type()));
        }
        Ok(action)
    }

    /// The unit type `name` of `file`, of the package at `index`, in the mode's scope, whose slots
    /// hold the package's `actions`: its AI, its kit, its slots, kind after kind, and its
    /// passive. An avatar's is tagged `avatar`, and stays when it dies.
    fn unit_type(
        &mut self,
        index: u16,
        name: &str,
        file: &UnitTypeFile,
        actions: &BTreeMap<&str, ActionId>,
        avatar: bool,
    ) -> Result<(), BookError> {
        let data = self.input.data;
        let books = &mut self.books;
        let unit_type = books
            .types
            .load(TypeScope::Mode, name, &file.core)
            .expect(CHECKED);
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
                        unit_type: name.to_owned(),
                        error,
                    }
                })?;
            books.ais.set(unit_type, ai);
        }
        let books = &mut self.books;
        let pools = file.pools.iter().map(|pool| {
            let id = PoolId::of(&data.pools, pool).expect(CHECKED);
            (id, &data.pools[pool].max)
        });
        let tracks = TrackSet::of(file.tracks.iter().map(|track| {
            let tracks = books.tracks.as_ref().expect(CHECKED);
            tracks.id(track).expect(CHECKED)
        }));
        let kit = UnitKit::new(file.stats.as_ref(), combat.as_ref(), pools, self.rules)
            .map_err(|error| BookError::Kit {
                package: index,
                unit_type: name.to_owned(),
                error,
            })?
            .with_vision(file.vision.as_ref())
            .with_body(data.navigation.body(file.collision.as_ref()))
            .with_tracks(tracks)
            .with_production(file.production.as_ref());
        let mut slots = Vec::new();
        for (kind, ids) in &file.slots {
            let kind = data.slots.named(kind.as_str()).expect(CHECKED);
            let slotted = ids.iter().map(|id| SlotAction {
                kind,
                ability: actions[id.as_str()],
            });
            slots.extend(slotted);
        }
        slots.sort_by_key(|action| action.kind);
        let passive = file.passive.as_ref().map(|passive| {
            books
                .modifiers
                .find(index, passive.as_str())
                .expect(CHECKED)
        });
        books.units.unit_types.push(UnitTypeSetup {
            unit_type,
            kit,
            stats: file.stats.clone().unwrap_or_default(),
            actions: slots,
            passive,
        });
        Ok(())
    }

    /// The projectile or area type `name` of `file`, of the package at `index`, in the scope its
    /// actions name types in, which only actions deliver, so the mode spawns none.
    fn delivery(
        &mut self,
        index: u16,
        name: &DeclaredName,
        file: &UnitTypeFile,
    ) -> Result<(), BookError> {
        let rate = self.input.rate;
        let books = &mut self.books;
        let scope = TypeScope::of_package(index);
        let unit_type = books
            .types
            .load(scope, name.as_str(), &file.core)
            .expect(CHECKED);
        if let Some(projectile) = &file.projectile {
            books.types.give_tag(unit_type, EngineTag::Projectile.tag());
            let spec = ProjectileSpec::of(projectile, &books.types, rate);
            books.projectiles.set(unit_type, spec);
            if projectile.homing {
                books.homing.push(unit_type);
            }
        }
        if let Some(area) = &file.area {
            books.types.give_tag(unit_type, EngineTag::Area.tag());
            let modifiers = &books.modifiers;
            let modifier = |id: &DeclaredName| modifiers.find(index, id.as_str()).expect(CHECKED);
            let spec = AreaSpec::of(area, &books.types, rate, modifier).ok_or_else(|| {
                BookError::AreaTime {
                    package: index,
                    unit_type: name.to_string(),
                }
            })?;
            books.areas.set(unit_type, spec);
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
    types: &'b UnitTypes,
    modifiers: &'b ModifierBook,
    tracks: Option<&'b TrackBook>,
    params: &'b ParamTable,
    /// The action whose effect lists resolve, with its params.
    action: Option<ActionId>,
    package: u16,
}

impl ActionNames for BuildNames<'_> {
    fn stat(&self, stat: &Stat) -> u16 {
        self.input.stat(stat)
    }

    fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
        self.input.damage_kind(name)
    }

    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        self.input.data.cost_target(name)
    }

    fn filter(&self, filter: &FilterData) -> Filter {
        Filter::resolve(filter, self.types).expect(CHECKED)
    }

    fn modifier(&self, package: u16, name: &DeclaredName) -> ModifierId {
        self.modifiers.find(package, name.as_str()).expect(CHECKED)
    }
}

impl EffectNames for BuildNames<'_> {
    fn param(&self, name: &DeclaredName) -> usize {
        let action = self.action.expect("an effect list is of an action");
        self.params
            .find(action.index(), name.as_str())
            .expect(CHECKED)
    }

    fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
        self.input.damage_kind(name)
    }

    fn pool(&self, name: &DeclaredName) -> PoolId {
        PoolId::of(&self.input.data.pools, name).expect(CHECKED)
    }

    fn modifier(&self, name: &DeclaredName) -> ModifierId {
        self.modifiers
            .find(self.package, name.as_str())
            .expect(CHECKED)
    }

    fn track(&self, name: &DeclaredName) -> TrackId {
        self.tracks
            .and_then(|tracks| tracks.id(name))
            .expect(CHECKED)
    }
}
