use std::iter;
use std::sync::Arc;

use campfire_capabilities::{BookInput, Books, PackageContent, PackageScript, Release};
use campfire_common::{MapName, PackageFingerprint};
use campfire_declare::{ScriptAt, SourceKind, SourcePackage};
use campfire_mode::{MapData, ModeData};
use campfire_script::{ScriptHost, ScriptId, ScriptUses};
use campfire_sim::TickRate;
use campfire_units::{HeightGrid, ScriptBook};
use campfire_values::{EngineTag, Names, PackageIndex, PackagePath, UnitType};

use crate::dependent::{Dependent, DependentKind};
use crate::error::load_problem::LoadProblem;
use crate::error::{ContentError, LoadError};
use crate::file_index::FileIndex;
use crate::files::map_file::MapFile;
use crate::files::mode_file::ModeFile;
use crate::files::mode_manifest::ModeManifest;
use crate::files::mode_script::ModeScript;
use crate::files::package_name::PackageName;
use crate::files::units_file::UnitsFile;
use crate::mode_packages::load_check::LoadCheck;
use crate::package::Package;
use crate::package_files::PackageFiles;
use crate::package_view::{PackageView, ViewKind};

mod load_check;

const MODE_DATA: &str = "data/mode.toml";
const UNITS_DATA: &str = "data/units.toml";

/// A mode and the packages it depends on, read and checked: what a match of the mode loads.
#[derive(Debug)]
pub struct ModePackages {
    pub(crate) mode: Package,
    pub(crate) manifest: ModeManifest,
    /// The script it runs as its own.
    pub(crate) script: ModeScript,
    pub(crate) data: ModeData,
    /// The name of the map it loaded, which its session plays.
    map_name: MapName,
    pub(crate) map: MapData,
    /// The map's height grid, `map/<name>/heights.bin`, when it has one.
    pub(crate) heights: Option<HeightGrid>,
    /// The mode's actions, modifiers, unit types and item types.
    pub(crate) content: PackageContent,
    /// In the order of their names in the mode's manifest.
    pub(crate) dependencies: Vec<Dependent>,
    /// The hooks each script defines, as the load read them, in the order a match compiles them.
    scripts: ScriptBook,
    /// What each script uses of the data, in the same order.
    uses: ScriptUses,
}

impl ModePackages {
    pub fn fingerprint(&self) -> PackageFingerprint {
        self.mode.fingerprint()
    }

    /// The fingerprints of its dependencies, in the order of their names in its manifest.
    pub fn dependency_fingerprints(
        &self,
    ) -> impl ExactSizeIterator<Item = PackageFingerprint> + '_ {
        self.dependencies
            .iter()
            .map(|dependent| dependent.package.fingerprint())
    }

    pub const fn manifest(&self) -> &ModeManifest {
        &self.manifest
    }

    pub const fn data(&self) -> &ModeData {
        &self.data
    }

    /// The mode's actions, modifiers, unit types and item types.
    pub const fn content(&self) -> &PackageContent {
        &self.content
    }

    /// The mode's own package.
    pub const fn mode(&self) -> PackageView<'_> {
        PackageView {
            index: PackageIndex::MODE,
            package: &self.mode,
            content: &self.content,
            kind: ViewKind::Mode,
        }
    }

    /// Every one of its packages, the mode's first, then each it depends on, in the order of
    /// their names in its manifest.
    pub fn packages(&self) -> impl Iterator<Item = PackageView<'_>> {
        let mode = self.mode();
        let dependents = self
            .dependencies
            .iter()
            .enumerate()
            .map(|(at, dependent)| PackageView {
                index: PackageIndex::dependency(at).expect("the load checked the package count"),
                package: &dependent.package,
                content: &dependent.content,
                kind: match &dependent.kind {
                    DependentKind::Avatar(avatar) => ViewKind::Avatar(avatar),
                    DependentKind::Loadout => ViewKind::Loadout,
                    DependentKind::Rules => ViewKind::Rules,
                },
            });
        iter::once(mode).chain(dependents)
    }

    /// The books of a match of its packages at `rate`, a rate within the manifest's range. The
    /// load built them at the fastest rate the range allows, where every time counts the most
    /// ticks, so they build at every rate a session may choose.
    pub fn books(&self, rate: TickRate) -> Books {
        assert!(
            self.manifest.tick_hz.contains(rate.hz()),
            "a session's rate is within the manifest's range"
        );
        let engine_tags = Release::engine_tags();
        Release::books(&self.book_input(rate, &engine_tags))
            .expect("the load built the books at the fastest rate")
    }

    /// The names of its unit types, each in its scope, as a match's books number them.
    pub fn unit_types(&self) -> Arc<Names<UnitType>> {
        let engine_tags = Release::engine_tags();
        let input = self.book_input(TickRate::new(self.manifest.tick_hz.fastest()), &engine_tags);
        let types = Release::unit_types(&input).expect("the load declared every type");
        Arc::clone(types.types())
    }

    /// Every script of its packages, in the order a match compiles them, which numbers each at
    /// its place, as its book of hooks holds it; the load parsed each one.
    pub fn scripts(&self) -> Vec<PackageScript<'_>> {
        let scripts = self.packages().flat_map(|view| &view.package.scripts);
        scripts
            .map(|script| PackageScript {
                path: &script.path,
                source: &script.source,
            })
            .collect()
    }

    /// The mode's script, by its place among the scripts a match compiles: past every script of
    /// the packages before the one that holds it.
    pub fn mode_script(&self) -> ScriptId {
        let mut before = 0;
        for view in self.packages() {
            if self.runs_script_of(view) {
                let at = view
                    .package
                    .script_index(self.script.path())
                    .expect("the load checked the mode's script");
                return ScriptId::nth(before + at);
            }
            before += view.package.scripts.len();
        }
        panic!("the load checked the package of the mode's script")
    }

    /// Whether `view` is the package that holds the mode's script.
    fn runs_script_of(&self, view: PackageView<'_>) -> bool {
        match (self.script.package(), view.kind) {
            (None, ViewKind::Mode) => true,
            (Some(name), ViewKind::Rules) => view.package.header.name == *name,
            _ => false,
        }
    }

    /// What its books are built from at `rate`.
    pub(crate) fn book_input<'a>(
        &'a self,
        rate: TickRate,
        engine_tags: &'a [EngineTag],
    ) -> BookInput<'a> {
        BookInput {
            data: &self.data,
            map: &self.map,
            heights: self.heights.as_ref(),
            teams: &self.manifest.teams,
            max_move_speed: self.manifest.max_move_speed,
            set: self.manifest.capabilities,
            engine_tags,
            packages: self.book_packages(),
            scripts: &self.scripts,
            script_uses: &self.uses,
            mode_script: self
                .packages()
                .find(|&view| self.runs_script_of(view))
                .map(|view| ScriptAt {
                    package: view.index,
                    path: self.script.path(),
                }),
            rate,
        }
    }

    /// Its packages as its books are built from them, the mode's then each dependency's.
    pub(crate) fn book_packages(&self) -> Vec<SourcePackage<'_>> {
        let packages = self.packages().map(|view| SourcePackage {
            name: view.package.header.name.as_str(),
            units: &view.content.units,
            content: &view.content.sections,
            kind: match view.kind {
                ViewKind::Mode => SourceKind::Mode,
                ViewKind::Avatar(avatar) => SourceKind::Avatar(&avatar.unit),
                ViewKind::Loadout => SourceKind::Loadout,
                ViewKind::Rules => SourceKind::Rules,
            },
            scripts: view
                .package
                .scripts
                .iter()
                .map(|script| &script.path)
                .collect(),
        });
        packages.collect()
    }

    /// The hooks each script of `packages` defines, in the order a match compiles them: the
    /// mode's scripts, then each dependency's, each package's in the order of their paths.
    fn read_hooks<'p>(packages: impl IntoIterator<Item = &'p Package>) -> ScriptBook {
        let mut book = ScriptBook::default();
        for script in packages.into_iter().flat_map(|package| &package.scripts) {
            let functions = script.facts.functions.iter();
            book.push(functions.map(|function| (function.name.as_str(), function.params)));
        }
        book
    }

    /// What each script of `packages` uses of the data, in the order a match compiles them.
    fn read_uses<'p>(packages: impl IntoIterator<Item = &'p Package>) -> ScriptUses {
        let mut uses = ScriptUses::default();
        for script in packages.into_iter().flat_map(|package| &package.scripts) {
            uses.push(&script.facts);
        }
        uses
    }

    /// The name of the map it loaded.
    pub const fn map_name(&self) -> &MapName {
        &self.map_name
    }

    pub const fn map(&self) -> &MapData {
        &self.map
    }

    /// The map's height grid, when it has one.
    pub const fn heights(&self) -> Option<&HeightGrid> {
        self.heights.as_ref()
    }

    /// The packages it depends on, in the order of their names in its manifest.
    pub fn dependencies(&self) -> &[Dependent] {
        &self.dependencies
    }

    /// The mode of `files`, whose manifest is `manifest`, on its map `map`, or its only map when
    /// `map` names none, with each package its manifest names in `dependencies`, in the order of
    /// their names: checked as a match loads them, each file the load reads parsed once. A mode
    /// of several maps and none named fails, its error naming them all.
    pub fn new(
        files: &PackageFiles,
        manifest: ModeManifest,
        map: Option<&MapName>,
        dependencies: &[(PackageName, &PackageFiles)],
    ) -> Result<Self, LoadError> {
        let mut parser = ScriptHost::new(manifest.script_limits.per_call);
        let api = Release::bind_script_api(&mut parser);
        let name = manifest.header.name.clone();
        let fail = |problem| LoadError::of(&name, problem);
        if PackageIndex::dependency(dependencies.len()).is_none() {
            return Err(fail(LoadProblem::TooManyPackages));
        }
        let ModeFile {
            script,
            data,
            mut content,
        } = files
            .read_data(&PackageFiles::engine_path(MODE_DATA))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let units: UnitsFile = files
            .read_data(&PackageFiles::engine_path(UNITS_DATA))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        content.units = units.units;
        let mut maps = maps_of(files.index()).map_err(fail)?;
        let map_name = match map {
            Some(map) if maps.contains(map) => map.clone(),
            Some(map) => return Err(fail(LoadProblem::NoMap(map.clone()))),
            None => match maps.pop() {
                Some(only) if maps.is_empty() => only,
                other => {
                    let named = maps.into_iter().chain(other).collect();
                    return Err(fail(LoadProblem::MapNeeded(named)));
                }
            },
        };
        let map_data = PackagePath::parse(&format!("map/{map_name}/map.toml"))
            .expect("a map's name is a package path's name");
        let MapFile(map) = files
            .reader()
            .read_data(&map_data)
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let heights = match files.reader().read_binary(&HeightGrid::path(&map_name)) {
            Ok(grid) => Some(grid),
            Err(ContentError::Missing { .. }) => None,
            Err(error) => return Err(fail(LoadProblem::Content(error))),
        };
        let mode = Package::read(files, &manifest.header, &parser, &api)?;
        let dependencies: Vec<Dependent> = dependencies
            .iter()
            .map(|(name, files)| Dependent::read(name, files, &parser, &api))
            .collect::<Result<_, _>>()?;
        if let Some(name) = script.package() {
            let rules = dependencies.iter().any(|dependent| {
                dependent.package.header.name == *name
                    && matches!(dependent.kind, DependentKind::Rules)
            });
            if !rules {
                return Err(fail(LoadProblem::ScriptPackage(name.clone())));
            }
        }
        let dependents = dependencies.iter().map(|dependent| &dependent.package);
        let scripts = Self::read_hooks(iter::once(&mode).chain(dependents.clone()));
        let uses = Self::read_uses(iter::once(&mode).chain(dependents));
        let packages = Self {
            mode,
            manifest,
            script,
            data,
            map_name,
            map,
            heights,
            content,
            dependencies,
            scripts,
            uses,
        };
        LoadCheck::run(&packages, &api)?;
        Ok(packages)
    }
}

/// The names of the maps of the mode whose index is `index`: each directory `map/<name>/` that
/// holds a `map.toml`, in order; one whose name no map can have is refused.
fn maps_of(index: &FileIndex) -> Result<Vec<MapName>, LoadProblem> {
    index
        .rows()
        .filter_map(|(path, _)| {
            let name = path
                .as_str()
                .strip_prefix("map/")?
                .strip_suffix("/map.toml")?;
            (!name.contains('/')).then_some(name)
        })
        .map(|name| MapName::parse(name).ok_or_else(|| LoadProblem::MapDirName(name.to_owned())))
        .collect()
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::Release;
    use campfire_sim::TickRate;

    use crate::mode_packages::ModePackages;

    impl ModePackages {
        /// Every tag its packages name but the engine's, in the order a match declares them,
        /// after the engine's.
        pub fn tag_names(&self) -> Vec<String> {
            let engine_tags = Release::engine_tags();
            let rate = TickRate::new(self.manifest.tick_hz.fastest());
            let types = Release::unit_types(&self.book_input(rate, &engine_tags));
            let types = types.expect("the load declared every tag");
            let names = types.tags().names().texts().skip(engine_tags.len());
            names.map(str::to_owned).collect()
        }
    }
}
