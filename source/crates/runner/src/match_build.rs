use std::collections::BTreeMap;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{
    Abilities, ActionData, ActionId, Actions, Areas, DeclaredName, KitRules, LoadoutSetup,
    MatchScripts, Mode, ModeSetup, OnDeath, Orders, PoolId, Progression, Projectiles, SlotAction,
    Stat, Stats, TypeScope, UnitKit, UnitTypeSetup, Units,
};
use campfire_content::PackagePath;
use campfire_package::{ModePackages, PackageView, UnitTypeFile, ViewKind};
use campfire_script::ScriptId;
use campfire_sim::{Capability, StateRegistry, TickRate};

use crate::error::StartError;

/// What the package load checked, which a match build trusts.
const CHECKED: &str = "the load checked it";

/// A match of a mode, built from its read packages into a world that `SimUpdate::prepare` set
/// up.
#[derive(Debug)]
pub(crate) struct MatchBuild<'a> {
    packages: &'a ModePackages,
    world: &'a mut World,
    rules: KitRules,
    /// Every unit type the mode spawns, as it loads: its projectile types are none.
    unit_types: Vec<UnitTypeSetup>,
    /// Every script of every package, compiled once: the mode's, then each dependency's, in the
    /// order of their packages' scripts.
    scripts: Vec<ScriptId>,
    /// Where each package's scripts start in `scripts`: the mode's, then each dependency's.
    script_starts: Vec<usize>,
    /// Each train and delivery loaded, and the name of the unit type it spawns in its package's
    /// scope, bound once all unit types load.
    spawns: Vec<(ActionId, &'a DeclaredName)>,
}

impl<'a> MatchBuild<'a> {
    /// Builds the match of `packages` for `players` players: the core and the declared
    /// capabilities the release has, the mode's unit types, its avatars and loadout, and the mode
    /// itself. A declared capability the release does not have yet installs nothing.
    pub(crate) fn run(
        packages: &'a ModePackages,
        world: &'a mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        players: u32,
    ) -> Result<(), StartError> {
        let manifest = packages.manifest();
        let scripts = MatchScripts {
            limits: manifest.script_limits,
            players,
            damage_kinds: packages.data().combat.damage_kinds.as_slice().into(),
            stats: packages.data().stats.keys().cloned().collect(),
            pools: packages.data().pools.keys().cloned().collect(),
            resources: packages.data().resources.as_slice().into(),
        };
        manifest
            .capabilities
            .install(world, schedule, registry, Some(scripts));
        let data = packages.data();
        let rules = KitRules {
            rate: *world.resource::<TickRate>(),
            max_move_speed: manifest.max_move_speed,
            life: data.combat.life_pool(&data.pools).unwrap_or(PoolId::FIRST),
        };
        let mut build = MatchBuild {
            packages,
            world,
            rules,
            unit_types: Vec::with_capacity(packages.content().units.len()),
            scripts: Vec::new(),
            script_starts: Vec::with_capacity(1 + packages.dependencies().len()),
            spawns: Vec::new(),
        };
        Units::declare_tags(build.world, packages.tag_names()).expect(CHECKED);
        if manifest.capabilities.contains(Capability::Progression) {
            Progression::load(build.world, &data.tracks);
        }
        build.compile_scripts();
        build.load_modifiers();
        let mut avatars = Vec::new();
        let mut loadout = Vec::new();
        let loadout_ranks = packages.data().loadout_ranks();
        for view in packages.packages() {
            let units = &view.content.units;
            match view.kind {
                ViewKind::Mode => {
                    let ranks = packages.slotted_ranks(units.values()).expect(CHECKED);
                    let actions = build.load_actions(view, |id| ranks.get(id).copied())?;
                    for (name, file) in units {
                        if file.delivers() {
                            build.load_delivery(view, name.as_str(), file);
                        } else {
                            build.load_unit_type(view, name.as_str(), file, &actions, false)?;
                        }
                    }
                }
                ViewKind::Avatar(avatar) => {
                    for (id, file) in units {
                        build.load_delivery(view, id.as_str(), file);
                    }
                    let name = &view.package.name;
                    let ranks = packages.slotted_ranks([&avatar.unit]).expect(CHECKED);
                    let actions = build.load_actions(view, |id| ranks.get(id).copied())?;
                    build.load_unit_type(view, name, &avatar.unit, &actions, true)?;
                    avatars.push(name.clone());
                }
                ViewKind::Loadout => {
                    for (id, file) in units {
                        build.load_delivery(view, id.as_str(), file);
                    }
                    let actions = build.load_actions(view, |_| Some(loadout_ranks))?;
                    let entries = actions.into_iter().map(|(id, ability)| LoadoutSetup {
                        id: id.to_owned(),
                        ability,
                    });
                    loadout.extend(entries);
                }
            }
        }
        let mode = packages.packages().next().expect("the mode is a package");
        for (action, unit_type) in &build.spawns {
            Actions::bind_spawn(build.world, *action, unit_type.as_str());
        }
        let setup = ModeSetup {
            script: build.script(mode, &packages.data().script),
            data: packages.data(),
            map: packages.map(),
            teams: &manifest.teams,
            players,
            unit_types: build.unit_types,
            avatars,
            loadout,
            walkers: packages.walkers(),
            max_move_speed: manifest.max_move_speed.get(),
            stat_order: packages.stat_graph().order().expect(CHECKED),
        };
        Mode::install(build.world, schedule, registry, setup).map_err(StartError::Mode)
    }

    /// Loads the modifiers of every package, the mode's first, before any ability names one.
    fn load_modifiers(&mut self) {
        for view in self.packages.packages() {
            for (name, data) in &view.content.modifiers {
                let script = data.script.as_ref().map(|path| self.script(view, path));
                Stats::load_modifier(self.world, view.index.get(), name.as_str(), data, script);
            }
        }
    }

    /// Loads the actions of `package`, each once, with the ranks `ranks` gives it, 1 when it
    /// gives none, by id.
    fn load_actions(
        &mut self,
        view: PackageView<'a>,
        ranks: impl Fn(&str) -> Option<u8>,
    ) -> Result<BTreeMap<&'a str, ActionId>, StartError> {
        view.content
            .actions
            .iter()
            .map(|(id, data)| {
                let ranks = ranks(id.as_str()).unwrap_or(1);
                Ok((
                    id.as_str(),
                    self.load_ability(view, id.as_str(), data, ranks)?,
                ))
            })
            .collect()
    }

    /// Loads the unit type `name` of `file`, of `package`, whose slots hold the package's
    /// loaded `actions`: its AI, its kit, its slots, kind after kind, and its passive. An
    /// avatar's is tagged `avatar`, and stays when it dies.
    fn load_unit_type(
        &mut self,
        view: PackageView<'a>,
        name: &str,
        file: &UnitTypeFile,
        actions: &BTreeMap<&str, ActionId>,
        avatar: bool,
    ) -> Result<(), StartError> {
        let data = self.packages.data();
        let mut combat = file.combat.clone();
        let unit_type =
            Units::load_type(self.world, TypeScope::Mode, name, &file.core).expect(CHECKED);
        if avatar {
            Units::tag_avatar(self.world, unit_type);
            if let Some(combat) = &mut combat {
                combat.on_death = OnDeath::Stay;
            }
        }
        let unit_error = |error| StartError::UnitKit {
            unit_type: name.to_owned(),
            error,
        };
        if let Some(orders) = &file.orders {
            let script = self.script(view, &orders.ai);
            Orders::load_ai(self.world, unit_type, orders, script).map_err(|error| {
                StartError::Ai {
                    unit_type: name.to_owned(),
                    error,
                }
            })?;
        }
        let pools = self.pools(&file.pools);
        let kit = UnitKit::new(file.stats.as_ref(), combat.as_ref(), pools, self.rules)
            .map_err(unit_error)?
            .with_vision(file.vision.as_ref())
            .with_body(data.navigation.body(file.collision.as_ref()))
            .with_tracks(Progression::tracks(self.world, &file.tracks))
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
        let id = view.index.get();
        let passive = file
            .passive
            .as_ref()
            .map(|passive| Stats::modifier(self.world, id, passive.as_str()).expect(CHECKED));
        self.unit_types.push(UnitTypeSetup {
            unit_type,
            kit,
            stats: file.stats.clone().unwrap_or_default(),
            actions: slots,
            passive,
        });
        Ok(())
    }

    /// Loads the projectile or area type `name` of `file`, of the package `view`, in the scope its
    /// actions name types in, which only actions deliver, so the mode spawns none.
    fn load_delivery(&mut self, view: PackageView<'a>, name: &str, file: &UnitTypeFile) {
        let scope = TypeScope::of_package(view.index.get());
        let unit_type = Units::load_type(self.world, scope, name, &file.core).expect(CHECKED);
        if let Some(projectile) = &file.projectile {
            Projectiles::load_type(self.world, unit_type, projectile);
        }
        if let Some(area) = &file.area {
            Areas::load_type(self.world, unit_type, view.index.get(), area);
        }
    }

    /// Each pool of `names`, which the load checked the mode declares, with the stat of its
    /// maximum.
    fn pools(
        &self,
        names: &'a [DeclaredName],
    ) -> impl Iterator<Item = (PoolId, &'a Stat)> + use<'a> {
        let pools = &self.packages.data().pools;
        names.iter().map(move |name| {
            let id = PoolId::of(pools, name).expect(CHECKED);
            (id, &pools[name].max)
        })
    }

    /// Loads the ability `id` of `package`, of `ranks` ranks, with its script; a train waits for
    /// its unit type to bind, and a delivery for its projectile or area type, of the same package.
    fn load_ability(
        &mut self,
        view: PackageView<'a>,
        id: &str,
        data: &'a ActionData,
        ranks: u8,
    ) -> Result<ActionId, StartError> {
        let script = data.script.as_ref().map(|path| self.script(view, path));
        let package = view.index.get();
        let action =
            Actions::load(self.world, package, id, data, script, ranks).map_err(|error| {
                StartError::Ability {
                    ability: id.to_owned(),
                    error,
                }
            })?;
        if !(data.on_resolve.is_empty() && data.on_hit.is_empty() && data.on_end.is_empty()) {
            Abilities::load_effects(self.world, action, package, data);
        }
        if let Some(unit_type) = &data.unit_type {
            self.spawns.push((action, unit_type));
        }
        if let Some(delivery) = &data.delivery {
            self.spawns.push((action, delivery.unit_type()));
        }
        Ok(action)
    }

    /// Compiles every script of every package in the match's host, each once.
    fn compile_scripts(&mut self) {
        for view in self.packages.packages() {
            self.script_starts.push(self.scripts.len());
            for script in &view.package.scripts {
                let id = Units::compile(self.world, &script.source).expect("the load parsed it");
                self.scripts.push(id);
            }
        }
    }

    /// The compiled script at `path` of the package `view`.
    fn script(&self, view: PackageView<'_>, path: &PackagePath) -> ScriptId {
        let at = view.package.script_index(path).expect(CHECKED);
        self.scripts[self.script_starts[usize::from(view.index.get())] + at]
    }
}
