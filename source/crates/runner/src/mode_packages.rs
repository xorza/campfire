use std::collections::BTreeMap;
use std::path::Path;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_capabilities::{
    Abilities, AbilityData, AbilityId, Combat, CombatData, Control, HeroData, HeroSetup, KitRules,
    Manifest, MapData, Mode, ModeData, ModeManifest, ModeSetup, Navigation, OnDeath, Projectiles,
    ResourcePool, SpellSetup, SpellsData, Stat, UnitKit, UnitKitError, UnitTypeData, UnitTypeSetup,
    Units, UnitsData,
};
use campfire_content::{ContentError, Fingerprint, PackageDir, PackagePath, PackageStore};
use campfire_script::ScriptHost;
use campfire_sim::{Capability, StateRegistry, TickRate};

use crate::error::{LoadError, LoadProblem, StartError};
use crate::load_check::LoadCheck;
use crate::package::Package;

const MODE_DATA: &str = "data/mode.toml";
const UNITS_DATA: &str = "data/units.toml";
const MAP_DATA: &str = "map/map.toml";
const HERO_DATA: &str = "data/hero.toml";
const SPELLS_DATA: &str = "data/spells.toml";

/// A capability's install.
type Install = fn(&mut World, &mut Schedule, &mut StateRegistry);

/// A mode and the packages it depends on, read and checked: what a match of the mode loads.
#[derive(Debug)]
pub struct ModePackages {
    pub(crate) mode: Package,
    pub(crate) manifest: ModeManifest,
    pub(crate) data: ModeData,
    pub(crate) units: UnitsData,
    pub(crate) map: MapData,
    /// In the order of their names in the mode's manifest.
    pub(crate) dependencies: Vec<Dependent>,
}

/// A package the mode depends on, and its data.
#[derive(Debug)]
pub(crate) struct Dependent {
    pub(crate) package: Package,
    pub(crate) content: Content,
}

#[derive(Debug)]
pub(crate) enum Content {
    Hero(Box<HeroData>),
    Spells(SpellsData),
}

impl ModePackages {
    /// The mode in `dir`, and its dependencies at the paths its manifest gives, as a workspace
    /// holds them.
    pub fn from_dir(dir: &Path) -> Result<ModePackages, LoadError> {
        let mode = PackageDir::new(dir);
        let manifest = read_mode_manifest(&mode)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| (name.clone(), PackageDir::new(dir.join(&dependency.path))))
            .collect::<Vec<_>>();
        ModePackages::assemble(&mode, manifest, &dependencies)
    }

    /// The mode of the fingerprint `mode`, and each dependency its manifest names by the
    /// fingerprint of the same place in `dependencies`, from `store`, as a verifier holds them.
    pub fn from_store(
        store: &PackageStore,
        mode: Fingerprint,
        dependencies: &[Fingerprint],
    ) -> Result<ModePackages, StartError> {
        let dir = store.get(mode).ok_or(StartError::UnknownMode)?.clone();
        let manifest = read_mode_manifest(&dir).map_err(StartError::Load)?;
        if manifest.dependencies.len() != dependencies.len() {
            return Err(StartError::DependencyCount);
        }
        let dependencies = manifest
            .dependencies
            .keys()
            .zip(dependencies)
            .map(|(name, &fingerprint)| {
                let dir = store
                    .get(fingerprint)
                    .ok_or_else(|| StartError::MissingDependency(name.clone()))?;
                Ok((name.clone(), dir.clone()))
            })
            .collect::<Result<Vec<_>, StartError>>()?;
        ModePackages::assemble(&dir, manifest, &dependencies).map_err(StartError::Load)
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.mode.fingerprint
    }

    /// The fingerprints of its dependencies, in the order of their names in its manifest.
    pub fn dependencies(&self) -> impl ExactSizeIterator<Item = Fingerprint> + '_ {
        self.dependencies
            .iter()
            .map(|dependent| dependent.package.fingerprint)
    }

    pub fn manifest(&self) -> &ModeManifest {
        &self.manifest
    }

    /// Loads the mode into `world`, which `SimUpdate::prepare` set up, for `players` players:
    /// the core and the declared capabilities the release has, the mode's unit types, its heroes
    /// and spells, and the mode itself. A declared capability the release does not have yet
    /// installs nothing.
    pub(crate) fn install(
        &self,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        players: u32,
    ) -> Result<(), StartError> {
        let manifest = &self.manifest;
        let declared = |capability| manifest.capabilities.contains(&capability);
        Units::install(world, schedule, registry, manifest.script_limits);
        let installs: [(Capability, Install); 5] = [
            (Capability::Combat, Combat::install),
            (Capability::Navigation, Navigation::install),
            (Capability::Projectiles, Projectiles::install),
            (Capability::Abilities, Abilities::install),
            (Capability::Orders, Control::install),
        ];
        for (capability, install) in installs {
            if declared(capability) {
                install(world, schedule, registry);
            }
        }
        let rules = KitRules {
            rate: *world.resource::<TickRate>(),
            max_move_speed: manifest
                .max_move_speed
                .to_num()
                .expect("the load checked the cap"),
        };
        let mut unit_types = Vec::with_capacity(self.units.units.len());
        for (name, file) in &self.units.units {
            let unit_type = Units::load_type(world, &file.core()).map_err(StartError::UnitType)?;
            if let Some(orders) = &file.orders {
                let source = &self
                    .mode
                    .script(&orders.ai)
                    .expect("the load checked it")
                    .source;
                Control::load_ai(world, unit_type, orders, source).map_err(|error| {
                    StartError::Ai {
                        unit_type: name.clone(),
                        error,
                    }
                })?;
            }
            let kit = UnitKit::new(file.stats.as_ref(), file.combat.as_ref(), rules).map_err(
                |error| StartError::UnitKit {
                    unit_type: name.clone(),
                    error,
                },
            )?;
            unit_types.push(UnitTypeSetup {
                name: name.clone(),
                unit_type,
                kit,
            });
        }
        let mut heroes = Vec::new();
        let mut spells = Vec::new();
        for dependent in &self.dependencies {
            let package = &dependent.package;
            match &dependent.content {
                Content::Hero(hero) => heroes.push(load_hero(world, package, hero, rules)?),
                Content::Spells(data) => {
                    for (id, ability) in &data.abilities {
                        spells.push(SpellSetup {
                            id: id.clone(),
                            ability: load_ability(world, package, id, ability)?,
                        });
                    }
                }
            }
        }
        let setup = ModeSetup {
            script: self
                .mode
                .script(&self.data.script)
                .expect("the load checked it")
                .source
                .clone(),
            data: self.data.clone(),
            map: self.map.clone(),
            teams: manifest.teams.clone(),
            players,
            unit_types,
            heroes,
            spells,
        };
        Mode::install(world, schedule, registry, setup).map_err(StartError::Mode)
    }

    /// Reads the mode's data and each dependency, and runs the load checks.
    fn assemble(
        dir: &PackageDir,
        manifest: ModeManifest,
        dependencies: &[(String, PackageDir)],
    ) -> Result<ModePackages, LoadError> {
        let parser = ScriptHost::new(manifest.script_limits.per_call);
        let name = manifest.name.clone();
        let fail = |problem| LoadError {
            package: name.clone(),
            problem: Box::new(problem),
        };
        let data = dir
            .read_data(&path(MODE_DATA))
            .map_err(content)
            .map_err(fail)?;
        let units = dir
            .read_data(&path(UNITS_DATA))
            .map_err(content)
            .map_err(fail)?;
        let map = dir
            .read_data(&path(MAP_DATA))
            .map_err(content)
            .map_err(fail)?;
        let mode = Package::read(dir, name.clone(), manifest.engine.clone(), &parser)?;
        let dependencies = dependencies
            .iter()
            .map(|(name, dir)| Dependent::read(name, dir, &parser))
            .collect::<Result<_, _>>()?;
        let packages = ModePackages {
            mode,
            manifest,
            data,
            units,
            map,
            dependencies,
        };
        LoadCheck::run(&packages)?;
        Ok(packages)
    }
}

impl Dependent {
    /// The package in `dir`, which the mode names `name`: a hero or spells package of that name.
    fn read(name: &str, dir: &PackageDir, parser: &ScriptHost) -> Result<Dependent, LoadError> {
        let fail = |problem| LoadError {
            package: name.to_owned(),
            problem: Box::new(problem),
        };
        let manifest: Manifest = dir
            .read_data(&path(PackageDir::MANIFEST))
            .map_err(content)
            .map_err(fail)?;
        if manifest.name() != name {
            return Err(fail(LoadProblem::OtherName(manifest.name().to_owned())));
        }
        let content = match &manifest {
            Manifest::Hero(_) => Content::Hero(Box::new(
                dir.read_data(&path(HERO_DATA))
                    .map_err(content)
                    .map_err(fail)?,
            )),
            Manifest::Spells(_) => {
                let data = dir.read_data(&path(SPELLS_DATA));
                Content::Spells(data.map_err(content).map_err(fail)?)
            }
            Manifest::Mode(_) => return Err(fail(LoadProblem::WrongKind)),
        };
        let package = Package::read(dir, name.to_owned(), manifest.engine().to_owned(), parser)?;
        Ok(Dependent { package, content })
    }
}

fn read_mode_manifest(dir: &PackageDir) -> Result<ModeManifest, LoadError> {
    let fail = |problem| LoadError {
        package: dir.root().display().to_string(),
        problem: Box::new(problem),
    };
    let manifest = dir.read_data(&path(PackageDir::MANIFEST));
    match manifest.map_err(content).map_err(fail)? {
        Manifest::Mode(manifest) => Ok(manifest),
        _ => Err(fail(LoadProblem::WrongKind)),
    }
}

/// Loads the hero `data` of `package`: its unit type, tagged `hero`, which stays when it dies; its
/// kit at level 1; its abilities, in slot order; and its resource pool.
fn load_hero(
    world: &mut World,
    package: &Package,
    data: &HeroData,
    rules: KitRules,
) -> Result<HeroSetup, StartError> {
    let core = UnitTypeData {
        tags: vec![UnitTypeData::HERO_TAG.to_owned()],
        params: BTreeMap::new(),
    };
    let unit_type = Units::load_type(world, &core).map_err(StartError::UnitType)?;
    let combat = CombatData {
        on_death: OnDeath::Stay,
        ..data.combat.clone()
    };
    let kit_error = |error| StartError::UnitKit {
        unit_type: package.name.clone(),
        error,
    };
    let kit = UnitKit::new(Some(&data.stats), Some(&combat), rules).map_err(kit_error)?;
    let abilities = data
        .slots
        .iter()
        .map(|id| load_ability(world, package, id, &data.abilities[id]))
        .collect::<Result<_, _>>()?;
    let resource = if data.stats.0.contains_key(&Stat::Resource) {
        let max = data.stats.at(Stat::Resource, 1);
        let max = max.ok_or_else(|| kit_error(UnitKitError::Overflow(Stat::Resource)))?;
        let pool = ResourcePool::new(max);
        Some(pool.ok_or_else(|| kit_error(UnitKitError::NotPositive(Stat::Resource)))?)
    } else {
        None
    };
    Ok(HeroSetup {
        id: package.name.clone(),
        unit_type,
        kit,
        abilities,
        resource,
    })
}

/// Loads the ability `id` of `package`, with its script's source.
fn load_ability(
    world: &mut World,
    package: &Package,
    id: &str,
    data: &AbilityData,
) -> Result<AbilityId, StartError> {
    let source = data.script.as_ref().map(|path| {
        package
            .script(path)
            .expect("the load checked that the package holds it")
            .source
            .as_str()
    });
    Abilities::load(world, data, source).map_err(|error| StartError::Ability {
        ability: id.to_owned(),
        error,
    })
}

/// One of the engine's paths in a package.
fn path(path: &str) -> PackagePath {
    PackagePath::parse(path).expect("the engine's paths are in the package")
}

fn content(error: ContentError) -> LoadProblem {
    LoadProblem::Content(error)
}
