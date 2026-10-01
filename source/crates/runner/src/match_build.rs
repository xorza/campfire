use std::collections::BTreeMap;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{
    Abilities, AbilityData, AbilityId, DeclaredName, KitRules, LoadoutSetup, MatchScripts, Mode,
    ModeSetup, OnDeath, Orders, PoolId, SlotAction, Stat, Stats, UnitKit, UnitTypeData,
    UnitTypeSetup, Units,
};
use campfire_content::PackagePath;
use campfire_package::{Content, ModePackages, Package, UnitTypeFile};
use campfire_script::ScriptId;
use campfire_sim::{StateRegistry, TickRate};

use crate::error::StartError;

/// The mode's place among the packages of a match build.
const MODE: usize = 0;
/// The place of the first dependency.
const DEPENDENCIES: usize = 1;

/// What the package load checked, which a match build trusts.
const CHECKED: &str = "the load checked it";

/// A match of a mode, built from its read packages into a world that `SimUpdate::prepare` set
/// up.
#[derive(Debug)]
pub(crate) struct MatchBuild<'a> {
    packages: &'a ModePackages,
    world: &'a mut World,
    rules: KitRules,
    /// Every unit type the mode spawns, as it loads.
    unit_types: Vec<UnitTypeSetup>,
    /// Every script of every package, compiled once: the mode's, then each dependency's, in the
    /// order of their packages' scripts.
    scripts: Vec<ScriptId>,
    /// Where each package's scripts start in `scripts`: the mode's, then each dependency's.
    script_starts: Vec<usize>,
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
            unit_types: Vec::with_capacity(packages.units().units.len()),
            scripts: Vec::new(),
            script_starts: Vec::with_capacity(1 + packages.dependencies().len()),
        };
        Units::declare_tags(build.world, packages.tag_names()).expect(CHECKED);
        build.compile_scripts();
        build.load_modifiers();
        let no_abilities = BTreeMap::new();
        for (name, file) in &packages.units().units {
            build.load_unit_type(MODE, name, file, &no_abilities, false)?;
        }
        let mut avatars = Vec::new();
        let mut loadout = Vec::new();
        let loadout_ranks = packages.data().loadout_ranks();
        for (at, dependent) in packages.dependencies().iter().enumerate() {
            let package = DEPENDENCIES + at;
            match &dependent.content {
                Content::Avatar(avatar) => {
                    let name = &dependent.package.name;
                    build.load_unit_type(package, name, &avatar.unit, &avatar.abilities, true)?;
                    avatars.push(name.clone());
                }
                Content::Loadout(data) => {
                    for (id, ability) in &data.abilities {
                        let ability = build.load_ability(package, id, ability, loadout_ranks)?;
                        loadout.push(LoadoutSetup {
                            id: id.clone(),
                            ability,
                        });
                    }
                }
            }
        }
        let setup = ModeSetup {
            script: build.script(MODE, &packages.data().script),
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
        let packages = self.packages;
        let dependents = packages
            .dependencies()
            .iter()
            .map(|dependent| match &dependent.content {
                Content::Avatar(avatar) => &avatar.modifiers,
                Content::Loadout(loadout) => &loadout.modifiers,
            });
        for (package, modifiers) in [&packages.data().modifiers]
            .into_iter()
            .chain(dependents)
            .enumerate()
        {
            let id = u16::try_from(package).expect("packages fit u16");
            for (name, data) in modifiers {
                let script = data.script.as_ref().map(|path| self.script(package, path));
                Stats::load_modifier(self.world, id, name, data, script);
            }
        }
    }

    /// Loads the unit type `name` of `file`, of `package`, whose `abilities` its actions name:
    /// its AI, its kit, its actions, each with the ranks of its slot kind, and its passive. An
    /// avatar's is tagged `avatar`, and stays when it dies.
    fn load_unit_type(
        &mut self,
        package: usize,
        name: &str,
        file: &UnitTypeFile,
        abilities: &BTreeMap<String, AbilityData>,
        avatar: bool,
    ) -> Result<(), StartError> {
        let data = self.packages.data();
        let mut core = file.core.clone();
        let mut combat = file.combat.clone();
        if avatar {
            core.tags.push(UnitTypeData::AVATAR_TAG.to_owned());
            if let Some(combat) = &mut combat {
                combat.on_death = OnDeath::Stay;
            }
        }
        let unit_type = Units::load_type(self.world, name, &core).expect(CHECKED);
        let unit_error = |error| StartError::UnitKit {
            unit_type: name.to_owned(),
            error,
        };
        if let Some(orders) = &file.orders {
            let script = self.script(package, &orders.ai);
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
            .with_body(data.navigation.body(file.collision.as_ref()));
        let mut actions = Vec::new();
        for (kind, ids) in &file.actions {
            let kind = data.slots.named(kind.as_str()).expect(CHECKED);
            for id in ids {
                let ranks = data.slots.ranks(kind);
                let ability = self.load_ability(package, id, &abilities[id], ranks)?;
                actions.push(SlotAction { kind, ability });
            }
        }
        actions.sort_by_key(|action| action.kind);
        let id = u16::try_from(package).expect("packages fit u16");
        let passive = file
            .passive
            .as_ref()
            .map(|passive| Stats::modifier(self.world, id, passive).expect(CHECKED));
        self.unit_types.push(UnitTypeSetup {
            unit_type,
            kit,
            stats: file.stats.clone().unwrap_or_default(),
            actions,
            passive,
        });
        Ok(())
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

    /// Loads the ability `id` of `package`, of `ranks` ranks, with its script.
    fn load_ability(
        &mut self,
        package: usize,
        id: &str,
        data: &AbilityData,
        ranks: u8,
    ) -> Result<AbilityId, StartError> {
        let script = data.script.as_ref().map(|path| self.script(package, path));
        let package = u16::try_from(package).expect("packages fit u16");
        Abilities::load(self.world, package, id, data, script, ranks).map_err(|error| {
            StartError::Ability {
                ability: id.to_owned(),
                error,
            }
        })
    }

    /// Compiles every script of every package in the match's host, each once.
    fn compile_scripts(&mut self) {
        let packages = self.packages;
        let dependencies = packages
            .dependencies()
            .iter()
            .map(|dependent| &dependent.package);
        for package in [packages.mode()].into_iter().chain(dependencies) {
            self.script_starts.push(self.scripts.len());
            for script in &package.scripts {
                let id = Units::compile(self.world, &script.source).expect("the load parsed it");
                self.scripts.push(id);
            }
        }
    }

    /// The compiled script at `path` of `package`: `MODE`, or `DEPENDENCIES` plus the place of
    /// a dependency.
    fn script(&self, package: usize, path: &PackagePath) -> ScriptId {
        let at = self.package(package).script_index(path).expect(CHECKED);
        self.scripts[self.script_starts[package] + at]
    }

    fn package(&self, package: usize) -> &'a Package {
        let packages = self.packages;
        match package.checked_sub(DEPENDENCIES) {
            None => packages.mode(),
            Some(at) => &packages.dependencies()[at].package,
        }
    }
}
