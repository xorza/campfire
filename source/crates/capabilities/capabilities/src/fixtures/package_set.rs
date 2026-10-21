use std::collections::BTreeMap;

use campfire_codec::{Toml, TomlTable};
use campfire_common::DeclaredName;
use campfire_declarations::{ScriptAt, SourceKind, SourcePackage, UnitDefaults, UnitTypeData};
use campfire_math::Num;
use campfire_mode::{MapData, ModeData, TeamManifest};
use campfire_script::{ScriptFacts, ScriptHost, ScriptLimits, ScriptUses};
use campfire_sim::{Capability, MatchRate};
use campfire_units::ScriptBook;
use campfire_values::{PackageIndex, PackagePath, Speed};

use crate::books::Books;
use crate::books::book_input::BookInput;
use crate::books::package_content::PackageContent;
use crate::release::Release;

/// A test's mode and the packages it depends on, as the text of their files, which a match loads
/// as a session's load does: the mode's `data/mode.toml`, which holds its content's actions and
/// modifiers and its unit types' defaults beside its own data; its `data/units.toml`; its map's
/// `map.toml`; each avatar's `data/avatar.toml` and each loadout's `data/loadout.toml`; and the
/// scripts of its list, in order, which its package holds as `scripts/<place>.rhai`.
#[derive(Debug, Clone)]
pub struct PackageSet {
    pub data: String,
    pub units: String,
    pub map: String,
    /// In the order of their names, as a manifest lists its dependencies.
    pub avatars: Vec<DependencyText>,
    pub loadouts: Vec<DependencyText>,
    pub scripts: Vec<String>,
    /// The place in `scripts` of the mode's own.
    pub own: usize,
    pub teams: Vec<TeamManifest>,
    /// The capabilities its manifest declares.
    pub capabilities: Vec<Capability>,
    pub rate: MatchRate,
}

/// A package a mode depends on: its name, and its one data file's text.
#[derive(Debug, Clone)]
pub struct DependencyText {
    pub name: String,
    pub data: String,
}

/// A package set as a match's load reads it, every script parsed.
#[derive(Debug)]
pub(crate) struct LoadedSet {
    pub(crate) data: ModeData,
    content: PackageContent,
    map: MapData,
    avatars: Vec<LoadedAvatar>,
    loadouts: Vec<PackageContent>,
    /// The path of each script of the mode's list, in its order.
    pub(crate) paths: Vec<PackagePath>,
    scripts: ScriptBook,
    uses: ScriptUses,
}

/// An avatar package as the load reads it: its one unit type, and its content.
#[derive(Debug)]
struct LoadedAvatar {
    unit: UnitTypeData,
    content: PackageContent,
}

impl PackageSet {
    /// The mode's data, as the load reads it, of which a test makes the host's picks.
    pub fn mode_data(&self) -> ModeData {
        let mut table = Self::table(&self.data);
        let defaults = Self::defaults(&mut table);
        Self::content(&mut table, &defaults);
        table
            .read_with(ModeData::seed(&Release::sections()))
            .unwrap()
    }

    /// The mode's map, as the load reads it.
    pub fn map_data(&self) -> MapData {
        Self::table(&self.map)
            .read_with(MapData::seed(&Release::sections()))
            .unwrap()
    }

    /// Every file of the set read, and every script parsed within `limits`, as the load reads
    /// them.
    pub(crate) fn load(&self, limits: ScriptLimits) -> LoadedSet {
        let readers = Release::sections();
        let mut table = Self::table(&self.data);
        let defaults = Self::defaults(&mut table);
        let mut content = Self::content(&mut table, &defaults);
        let data = table.read_with(ModeData::seed(&readers)).unwrap();
        content.units = Self::table(&self.units)
            .take::<TomlTable>("units")
            .map_or_else(BTreeMap::new, |units| {
                let units = units.unwrap();
                let units = units.into_entries().map(|(name, unit)| {
                    let unit = unit.read::<TomlTable>().unwrap();
                    let unit = unit.read_with(UnitTypeData::seed(&readers, &defaults));
                    (DeclaredName::new(&name).unwrap(), unit.unwrap())
                });
                units.collect()
            });
        let map = self.map_data();
        let avatars = self.avatars.iter().map(|avatar| {
            // The name players see, which the load reads apart.
            let mut table = Self::table(&avatar.data);
            table.take_value("name");
            let content = Self::content(&mut table, &defaults);
            let unit = table.read_with(UnitTypeData::seed(&readers, &defaults));
            LoadedAvatar {
                unit: unit.unwrap(),
                content,
            }
        });
        let loadouts = self.loadouts.iter().map(|loadout| {
            Self::table(&loadout.data)
                .read_with(PackageContent::seed(&readers, &defaults))
                .unwrap()
        });
        let mut host = ScriptHost::new(limits.per_call);
        let api = Release::bind_script_api(&mut host);
        let mut scripts = ScriptBook::new(api.hooks().iter().map(|status| status.hook));
        let mut uses = ScriptUses::default();
        for source in &self.scripts {
            let facts = ScriptFacts::read(&host, source, &api).unwrap();
            let functions = facts.functions.iter();
            scripts.push(functions.map(|function| (function.name.as_str(), function.params)));
            uses.push(&facts);
        }
        let paths = (0..self.scripts.len()).map(|place| {
            PackagePath::parse(&format!("scripts/{place}.rhai")).expect("a script's path")
        });
        LoadedSet {
            data,
            content,
            map,
            avatars: avatars.collect(),
            loadouts: loadouts.collect(),
            paths: paths.collect(),
            scripts,
            uses,
        }
    }

    fn table(text: &str) -> TomlTable {
        Toml::parse(text).unwrap()
    }

    /// Takes the defaults of the unit types' sections out of `table`, the mode's data.
    fn defaults(table: &mut TomlTable) -> UnitDefaults {
        table
            .take_value("defaults")
            .map_or_else(UnitDefaults::default, |value| {
                UnitDefaults::read(value, &Release::sections()).unwrap()
            })
    }

    /// Takes the content's keys out of `table`, a data file's, and reads them, `defaults` under
    /// its unit types' sections.
    fn content(table: &mut TomlTable, defaults: &UnitDefaults) -> PackageContent {
        let readers = Release::sections();
        let content = table.take_keys(PackageContent::keys(&readers));
        content
            .read_with(PackageContent::seed(&readers, defaults))
            .unwrap()
    }
}

impl LoadedSet {
    /// The books of the match of `set`, which it loaded, as a session's load builds them.
    pub(crate) fn books(&self, set: &PackageSet) -> Books {
        let mode = SourcePackage {
            name: "mode",
            kind: SourceKind::Mode,
            units: &self.content.units,
            content: &self.content.sections,
            scripts: self.paths.iter().collect(),
        };
        let avatars = set
            .avatars
            .iter()
            .zip(&self.avatars)
            .map(|(text, avatar)| SourcePackage {
                name: &text.name,
                kind: SourceKind::Avatar(&avatar.unit),
                units: &avatar.content.units,
                content: &avatar.content.sections,
                scripts: Vec::new(),
            });
        let loadouts = set
            .loadouts
            .iter()
            .zip(&self.loadouts)
            .map(|(text, content)| SourcePackage {
                name: &text.name,
                kind: SourceKind::Loadout,
                units: &content.units,
                content: &content.sections,
                scripts: Vec::new(),
            });
        let mode_scripts = self.paths.iter().map(|path| ScriptAt {
            package: PackageIndex::MODE,
            path,
        });
        let input = BookInput {
            data: &self.data,
            map: &self.map,
            heights: None,
            teams: &set.teams,
            max_move_speed: Speed::new(Num::MAX).unwrap(),
            set: Release::capability_set(&set.capabilities).unwrap(),
            engine_tags: &Release::engine_tags(),
            packages: [mode].into_iter().chain(avatars).chain(loadouts).collect(),
            scripts: &self.scripts,
            script_uses: &self.uses,
            mode_scripts: mode_scripts.collect(),
            tick_rate: set.rate,
        };
        Release::books(&input).unwrap()
    }
}
