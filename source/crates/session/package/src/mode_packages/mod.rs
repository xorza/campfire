use std::collections::BTreeMap;
use std::iter;
use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};

use campfire_capabilities::{BookInput, Books, PackageContent, PackageScript, Release};
use campfire_common::{DeclaredName, MapName, PackageFingerprint};
use campfire_declarations::{ScriptAt, SourceKind, SourcePackage};
use campfire_mode::{MapData, ModeData, ModeParam};
use campfire_package_format::{ContentError, FileIndex, PackageFiles, PackageFinder, PackagePlace};
use campfire_script::{ModeScripts, ScriptId};
use campfire_sim::MatchRate;
use campfire_units::HeightGrid;
use campfire_values::{EngineTag, PackageIndex, PackagePath};

use crate::data_override::DataOverride;
use crate::dependent::{Dependent, DependentKind};
use crate::error::load_problem::LoadProblem;
use crate::error::{LoadError, StoreError};
use crate::files::map_file::MapFile;
use crate::files::mode_manifest::ModeManifest;
use crate::mode_packages::load_check::LoadCheck;
use crate::mode_packages::mode_base::ModeBase;
use crate::mode_packages::mode_read::ModeRead;
use crate::package_view::{PackageView, ViewKind};
use campfire_values::PackageName;
use campfire_values::ScriptSource;

mod load_check;
pub(crate) mod mode_base;
pub(crate) mod mode_read;

/// A mode and the packages it depends on, read and checked: what a match of the mode loads.
#[derive(Debug)]
pub struct ModePackages {
    /// What it is on any of its maps, which each map's load shares.
    pub(crate) base: Arc<ModeBase>,
    /// Each script of its list, in its order, found once as the load checked it.
    listed: Vec<ListedScript>,
    /// The script of each of its bots, by the bot's name.
    bot_scripts: BTreeMap<DeclaredName, ScriptId>,
    /// The books its load built, at the fastest rate, kept for what reads their names alone, as
    /// a book's rate changes no name.
    load_books: OnceLock<Arc<Books>>,
    /// The name of the map it loaded, which its session plays.
    map_name: MapName,
    pub(crate) map: MapData,
    /// The map's height grid, `map/<name>/heights.bin`, when it has one.
    pub(crate) heights: Option<HeightGrid>,
}

/// A script of the mode's list: the package that holds it, and its place among the scripts a
/// match compiles.
#[derive(Debug, Clone, Copy)]
struct ListedScript {
    package: PackageIndex,
    id: ScriptId,
}

/// A bot a mode names, as a match runs it: its script, by its place among the scripts a match
/// compiles, the period of its think, and its params, which lie over the mode's.
#[derive(Debug, Clone, Copy)]
pub struct ModeBot<'a> {
    pub script: ScriptId,
    pub think_ms: NonZeroU32,
    pub params: &'a BTreeMap<DeclaredName, ModeParam>,
}

impl ModePackages {
    pub fn fingerprint(&self) -> PackageFingerprint {
        self.base.mode.fingerprint()
    }

    /// The fingerprints of its dependencies, in the order of their names in its manifest.
    pub fn dependency_fingerprints(
        &self,
    ) -> impl ExactSizeIterator<Item = PackageFingerprint> + '_ {
        self.base
            .dependencies
            .iter()
            .map(|dependent| dependent.package.fingerprint())
    }

    pub fn manifest(&self) -> &ModeManifest {
        &self.base.manifest
    }

    pub fn data(&self) -> &ModeData {
        &self.base.data
    }

    /// The mode's actions, modifiers, unit types and item types.
    pub fn content(&self) -> &PackageContent {
        &self.base.content
    }

    /// The mode's own package.
    pub fn mode(&self) -> PackageView<'_> {
        PackageView {
            index: PackageIndex::MODE,
            package: &self.base.mode,
            content: &self.base.content,
            kind: ViewKind::Mode,
        }
    }

    /// Every one of its packages, the mode's first, then each it depends on, in the order of
    /// their names in its manifest.
    pub fn packages(&self) -> impl Iterator<Item = PackageView<'_>> {
        let mode = self.mode();
        let dependents = self
            .base
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
    pub fn books(&self, rate: MatchRate) -> Books {
        assert!(
            self.base.manifest.tick_hz.contains(rate.hz()),
            "a session's rate is within the manifest's range"
        );
        let engine_tags = Release::engine_tags();
        Release::books(&self.book_input(rate, engine_tags))
            .expect("the load built the books at the fastest rate")
    }

    /// The books its load built, at the fastest rate, for what reads their names alone, as a
    /// bot's script does: a book's rate changes its times, never its names.
    pub fn load_books(&self) -> &Arc<Books> {
        self.load_books
            .get()
            .expect("the load keeps the books it built")
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

    /// The bot `name`: its script, by its place among the scripts a match compiles, the period of
    /// its think, and its params; none for a bot the mode does not name.
    pub fn bot(&self, name: &DeclaredName) -> Option<ModeBot<'_>> {
        let bot = self.base.bots.get(name)?;
        Some(ModeBot {
            script: self.bot_scripts[name],
            think_ms: bot.think_ms,
            params: &bot.params,
        })
    }

    /// The scripts the mode's list names, in its order, each by its place among the scripts a
    /// match compiles.
    pub fn mode_scripts(&self) -> ModeScripts {
        let ids = self.listed.iter().map(|listed| listed.id);
        ModeScripts::new(ids, self.base.list.mode())
    }

    /// The script `source` of the mode's list or of a bot, found once its load checked it: the
    /// package that holds it, and its place among the scripts a match compiles, past every
    /// script of the packages before.
    fn find_script(&self, source: &ScriptSource) -> ListedScript {
        let mut before = 0;
        for view in self.packages() {
            if view.holds(source) {
                let at = view
                    .package
                    .script_index(source.path())
                    .expect("the load checked each script of the mode's list and its bots");
                return ListedScript {
                    package: view.index,
                    id: ScriptId::nth(before + at),
                };
            }
            before += view.package.scripts.len();
        }
        panic!("the load checked the package of each script of the mode's list and its bots")
    }

    /// The paths of the scripts of the mode's list that the package at `view` holds.
    pub(crate) fn listed_in<'s>(
        &'s self,
        view: PackageView<'s>,
    ) -> impl Iterator<Item = &'s PackagePath> {
        let held = self
            .base
            .list
            .iter()
            .filter(move |source| view.holds(source));
        held.map(ScriptSource::path)
    }

    /// What its books are built from at `rate`.
    pub(crate) fn book_input<'a>(
        &'a self,
        rate: MatchRate,
        engine_tags: &'a [EngineTag],
    ) -> BookInput<'a> {
        BookInput {
            data: &self.base.data,
            map: &self.map,
            heights: self.heights.as_ref(),
            teams: &self.base.manifest.teams,
            max_move_speed: self.base.manifest.max_move_speed,
            set: self.base.manifest.capabilities,
            engine_tags,
            packages: self.book_packages(),
            scripts: &self.base.scripts,
            script_uses: &self.base.uses,
            mode_scripts: self
                .base
                .list
                .iter()
                .zip(&self.listed)
                .map(|(source, listed)| ScriptAt {
                    package: listed.package,
                    path: source.path(),
                })
                .collect(),
            tick_rate: rate,
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
        &self.base.dependencies
    }

    /// The mode at `place` on its map `map`, or its only map when `map` names none, with each
    /// package its manifest names at its path from there, each read once.
    pub fn at(place: &impl PackagePlace, map: Option<&MapName>) -> Result<Self, LoadError> {
        ModeRead::at(place)?.on(map)
    }

    /// The mode of the fingerprint `mode` that `finder` holds on its map `map`, with each
    /// dependency its manifest names by the fingerprint of the same place in `dependencies`, as a
    /// verifier holds them.
    pub fn found(
        finder: &impl PackageFinder,
        mode: PackageFingerprint,
        map: &MapName,
        dependencies: &[PackageFingerprint],
    ) -> Result<Self, StoreError> {
        ModeRead::found(finder, mode, dependencies)?
            .on(Some(map))
            .map_err(StoreError::Load)
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
        let base = ModeBase::read(files, manifest, dependencies)?;
        Self::on_map(Arc::new(base), files, map)
    }

    /// The mode `base`, whose files are `files`, on its map `map`, or its only map when `map`
    /// names none: its map's files read, and the load's checks and books run on it.
    pub(crate) fn on_map(
        base: Arc<ModeBase>,
        files: &PackageFiles,
        map: Option<&MapName>,
    ) -> Result<Self, LoadError> {
        let fail = |problem| LoadError::of(&base.manifest.header.name, problem);
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
        let map_data = MapFile::path(&map_name);
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
        // The scripts of the list and of the bots are found once the load checked each.
        let mut packages = Self {
            base,
            listed: Vec::new(),
            bot_scripts: BTreeMap::new(),
            load_books: OnceLock::new(),
            map_name,
            map,
            heights,
        };
        let books = LoadCheck::run(&packages, &packages.base.api)?;
        packages
            .load_books
            .set(Arc::new(books))
            .expect("the load builds its books once");
        packages.listed = packages
            .base
            .list
            .iter()
            .map(|source| packages.find_script(source))
            .collect();
        packages.bot_scripts = (&packages.base.bots)
            .into_iter()
            .map(|(name, bot)| (name.clone(), packages.find_script(&bot.script).id))
            .collect();
        Ok(packages)
    }

    /// Each record of mode data more than one of its packages gives, with the package whose value
    /// the mode takes, in the order the load laid them.
    pub fn overrides(&self) -> &[DataOverride] {
        &self.base.overrides
    }
}

/// The names of the maps of the mode whose index is `index`: each directory `map/<name>/` that
/// holds a `map.toml`, in order; one whose name no map can have is refused.
fn maps_of(index: &FileIndex) -> Result<Vec<MapName>, LoadProblem> {
    index
        .paths()
        .filter_map(MapFile::map_of)
        .map(|name| MapName::parse(name).ok_or_else(|| LoadProblem::MapDirName(name.to_owned())))
        .collect()
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::Release;
    use campfire_sim::MatchRate;
    use campfire_values::PackagePath;

    use std::sync::Arc;

    use crate::mode_packages::ModePackages;

    impl ModePackages {
        /// Writes the script at `path` of the mode's own package as `source`, past the load's
        /// check of it, as a fault of that check would let it pass, so a test meets the guards a
        /// match keeps at run time; `source` defines the hooks the load read of the script.
        pub fn write_unchecked(&mut self, path: &PackagePath, source: &str) {
            let base =
                Arc::get_mut(&mut self.base).expect("a test's packages hold their base alone");
            let script = (base.mode.scripts.iter_mut())
                .find(|script| &script.path == path)
                .expect("a script of the mode's own package");
            source.clone_into(&mut script.source);
        }

        /// Every tag its packages name but the engine's, in the order a match declares them,
        /// after the engine's.
        pub fn tag_names(&self) -> Vec<String> {
            let engine_tags = Release::engine_tags();
            let rate = MatchRate::new(self.base.manifest.tick_hz.fastest());
            let types = Release::unit_types(&self.book_input(rate, engine_tags));
            let types = types.expect("the load declared every tag");
            let names = types.tags().names().texts().skip(engine_tags.len());
            names.map(str::to_owned).collect()
        }
    }
}
