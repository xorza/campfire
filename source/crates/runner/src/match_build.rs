use std::collections::BTreeMap;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{
    Abilities, AbilityData, AbilityId, AvatarSetup, CombatData, EngineStat, KitRules, LoadoutSetup,
    MatchScripts, Mode, ModeSetup, OnDeath, Orders, ResourcePool, Stats, UnitKit, UnitKitError,
    UnitTypeData, UnitTypeSetup, Units,
};
use campfire_content::PackagePath;
use campfire_package::{AvatarData, Content, LoadoutData, ModePackages, Package};
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
            damage_kinds: packages.data().damage_kinds.as_slice().into(),
        };
        manifest
            .capabilities
            .install(world, schedule, registry, Some(scripts));
        let rules = KitRules {
            rate: *world.resource::<TickRate>(),
            max_move_speed: manifest.max_move_speed,
        };
        let mut build = MatchBuild {
            packages,
            world,
            rules,
            unit_types: Vec::with_capacity(packages.units().units.len()),
            scripts: Vec::new(),
            script_starts: Vec::with_capacity(1 + packages.dependencies().len()),
        };
        build.compile_scripts();
        build.load_unit_types()?;
        build.load_modifiers();
        let mut avatars = Vec::new();
        let mut loadout = Vec::new();
        for (at, dependent) in packages.dependencies().iter().enumerate() {
            let package = DEPENDENCIES + at;
            match &dependent.content {
                Content::Avatar(avatar) => avatars.push(build.load_avatar(package, avatar)?),
                Content::Loadout(data) => {
                    for (id, ability) in &data.abilities {
                        let ability =
                            build.load_ability(package, id, ability, LoadoutData::RANKS)?;
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
            walkers: packages.walker_radii(),
            max_move_speed: manifest.max_move_speed.get(),
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
            let package = u16::try_from(package).expect("packages fit u16");
            for (name, data) in modifiers {
                Stats::load_modifier(self.world, package, name, data);
            }
        }
    }

    /// Loads the mode's unit types, each with its AI and its kit.
    fn load_unit_types(&mut self) -> Result<(), StartError> {
        let packages = self.packages;
        for (name, file) in &packages.units().units {
            let unit_type = Units::load_type(self.world, name, &file.core).expect(CHECKED);
            if let Some(orders) = &file.orders {
                let script = self.script(MODE, &orders.ai);
                Orders::load_ai(self.world, unit_type, orders, script).map_err(|error| {
                    StartError::Ai {
                        unit_type: name.clone(),
                        error,
                    }
                })?;
            }
            let kit = UnitKit::new(file.stats.as_ref(), file.combat.as_ref(), self.rules)
                .map(|kit| {
                    kit.with_vision(file.vision.as_ref())
                        .with_collision(file.collision.as_ref())
                })
                .map_err(|error| StartError::UnitKit {
                    unit_type: name.clone(),
                    error,
                })?;
            let stats = file.stats.clone().unwrap_or_default();
            self.unit_types.push(UnitTypeSetup {
                unit_type,
                kit,
                stats,
            });
        }
        Ok(())
    }

    /// Loads the avatar `data` of `package`: its unit type, named for the package and tagged
    /// `avatar`, which stays when it dies, with its kit at level 1; its abilities, in slot order;
    /// and its resource pool.
    fn load_avatar(
        &mut self,
        package: usize,
        data: &AvatarData,
    ) -> Result<AvatarSetup, StartError> {
        let name = &self.package(package).name;
        let core = UnitTypeData {
            tags: vec![UnitTypeData::AVATAR_TAG.to_owned()],
            params: BTreeMap::new(),
        };
        let unit_type = Units::load_type(self.world, name, &core).expect(CHECKED);
        let combat = CombatData {
            on_death: OnDeath::Stay,
            ..data.combat.clone()
        };
        let kit_error = |error| StartError::UnitKit {
            unit_type: name.clone(),
            error,
        };
        let kit = UnitKit::new(Some(&data.stats), Some(&combat), self.rules)
            .map_err(kit_error)?
            .with_vision(data.vision.as_ref())
            .with_collision(data.collision.as_ref());
        let abilities = data
            .slots
            .iter()
            .enumerate()
            .map(|(slot, id)| {
                let ranks = AvatarData::slot_ranks(slot);
                self.load_ability(package, id, &data.abilities[id], ranks)
            })
            .collect::<Result<_, _>>()?;
        let resource = if data.stats.declares(EngineStat::Resource) {
            let max = data.stats.at(EngineStat::Resource, 1);
            let max = max.ok_or_else(|| kit_error(UnitKitError::Overflow(EngineStat::Resource)))?;
            let pool = ResourcePool::new(max);
            Some(pool.ok_or_else(|| kit_error(UnitKitError::NotPositive(EngineStat::Resource)))?)
        } else {
            None
        };
        self.unit_types.push(UnitTypeSetup {
            unit_type,
            kit,
            stats: data.stats.clone(),
        });
        Ok(AvatarSetup {
            id: name.clone(),
            unit_type,
            abilities,
            resource,
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
