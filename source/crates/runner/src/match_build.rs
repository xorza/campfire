use std::collections::BTreeMap;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{
    Abilities, AbilityData, AbilityId, CombatData, Control, HeroData, HeroSetup, KitRules,
    MatchScripts, Mode, ModeSetup, OnDeath, ResourcePool, SpellSetup, SpellsData, Stat, UnitKit,
    UnitKitError, UnitTypeData, UnitTypeSetup, Units,
};
use campfire_sim::{StateRegistry, TickRate};

use crate::error::StartError;
use crate::mode_packages::{Content, ModePackages};
use crate::package::Package;

/// A match of a mode, built from its read packages into a world that `SimUpdate::prepare` set
/// up.
#[derive(Debug)]
pub(crate) struct MatchBuild<'a> {
    packages: &'a ModePackages,
    world: &'a mut World,
    rules: KitRules,
    /// Every unit type the mode spawns, as it loads.
    unit_types: Vec<UnitTypeSetup>,
}

impl<'a> MatchBuild<'a> {
    /// Builds the match of `packages` for `players` players: the core and the declared
    /// capabilities the release has, the mode's unit types, its heroes and spells, and the mode
    /// itself. A declared capability the release does not have yet installs nothing.
    pub(crate) fn run(
        packages: &'a ModePackages,
        world: &'a mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        players: u32,
    ) -> Result<(), StartError> {
        let manifest = &packages.manifest;
        let scripts = MatchScripts {
            limits: manifest.script_limits,
            players,
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
            unit_types: Vec::with_capacity(packages.units.units.len()),
        };
        build.load_unit_types()?;
        let mut heroes = Vec::new();
        let mut spells = Vec::new();
        for dependent in &packages.dependencies {
            let package = &dependent.package;
            match &dependent.content {
                Content::Hero(hero) => heroes.push(build.load_hero(package, hero)?),
                Content::Spells(data) => {
                    for (id, ability) in &data.abilities {
                        let ability =
                            build.load_ability(package, id, ability, SpellsData::RANKS)?;
                        spells.push(SpellSetup {
                            id: id.clone(),
                            ability,
                        });
                    }
                }
            }
        }
        let script = packages
            .mode
            .script(&packages.data.script)
            .expect("the load checked it");
        let setup = ModeSetup {
            script: &script.source,
            data: &packages.data,
            map: &packages.map,
            teams: &manifest.teams,
            players,
            unit_types: build.unit_types,
            heroes,
            spells,
        };
        Mode::install(build.world, schedule, registry, setup).map_err(StartError::Mode)
    }

    /// Loads the mode's unit types, each with its AI and its kit.
    fn load_unit_types(&mut self) -> Result<(), StartError> {
        let packages = self.packages;
        for (name, file) in &packages.units.units {
            let unit_type =
                Units::load_type(self.world, name, &file.core()).map_err(StartError::UnitType)?;
            if let Some(orders) = &file.orders {
                let source = &packages
                    .mode
                    .script(&orders.ai)
                    .expect("the load checked it")
                    .source;
                Control::load_ai(self.world, unit_type, orders, source).map_err(|error| {
                    StartError::Ai {
                        unit_type: name.clone(),
                        error,
                    }
                })?;
            }
            let kit = UnitKit::new(file.stats.as_ref(), file.combat.as_ref(), self.rules).map_err(
                |error| StartError::UnitKit {
                    unit_type: name.clone(),
                    error,
                },
            )?;
            self.unit_types.push(UnitTypeSetup { unit_type, kit });
        }
        Ok(())
    }

    /// Loads the hero `data` of `package`: its unit type, named for the package and tagged
    /// `hero`, which stays when it dies, with its kit at level 1; its abilities, in slot order;
    /// and its resource pool.
    fn load_hero(&mut self, package: &Package, data: &HeroData) -> Result<HeroSetup, StartError> {
        let core = UnitTypeData {
            tags: vec![UnitTypeData::HERO_TAG.to_owned()],
            params: BTreeMap::new(),
        };
        let unit_type =
            Units::load_type(self.world, &package.name, &core).map_err(StartError::UnitType)?;
        let combat = CombatData {
            on_death: OnDeath::Stay,
            ..data.combat.clone()
        };
        let kit_error = |error| StartError::UnitKit {
            unit_type: package.name.clone(),
            error,
        };
        let kit = UnitKit::new(Some(&data.stats), Some(&combat), self.rules).map_err(kit_error)?;
        let abilities = data
            .slots
            .iter()
            .enumerate()
            .map(|(slot, id)| {
                let ranks = HeroData::slot_ranks(slot);
                self.load_ability(package, id, &data.abilities[id], ranks)
            })
            .collect::<Result<_, _>>()?;
        let resource = if data.stats.0.contains_key(&Stat::Resource) {
            let max = data.stats.at(Stat::Resource, 1);
            let max = max.ok_or_else(|| kit_error(UnitKitError::Overflow(Stat::Resource)))?;
            let pool = ResourcePool::new(max);
            Some(pool.ok_or_else(|| kit_error(UnitKitError::NotPositive(Stat::Resource)))?)
        } else {
            None
        };
        self.unit_types.push(UnitTypeSetup { unit_type, kit });
        Ok(HeroSetup {
            id: package.name.clone(),
            unit_type,
            abilities,
            resource,
        })
    }

    /// Loads the ability `id` of `package`, of `ranks` ranks, with its script's source.
    fn load_ability(
        &mut self,
        package: &Package,
        id: &str,
        data: &AbilityData,
        ranks: u8,
    ) -> Result<AbilityId, StartError> {
        let source = data.script.as_ref().map(|path| {
            package
                .script(path)
                .expect("the load checked that the package holds it")
                .source
                .as_str()
        });
        Abilities::load(self.world, data, source, ranks).map_err(|error| StartError::Ability {
            ability: id.to_owned(),
            error,
        })
    }
}
