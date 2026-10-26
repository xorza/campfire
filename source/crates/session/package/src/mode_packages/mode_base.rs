use std::iter;

use campfire_capabilities::{PackageContent, Release};
use campfire_codec::TomlTable;
use campfire_mode::ModeData;
use campfire_package_format::{ContentError, PackageFiles};
use campfire_script::{ScriptApi, ScriptHost, ScriptUses};
use campfire_sim::Capability;
use campfire_units::ScriptBook;
use campfire_values::PackageIndex;

use crate::data_override::DataOverride;
use crate::dependent::{Dependent, DependentKind, OpenedDependent};
use crate::error::LoadError;
use crate::error::load_problem::LoadProblem;
use crate::files::bot_list::BotList;
use crate::files::mode_file::ModeFile;
use crate::files::mode_manifest::ModeManifest;
use crate::files::script_list::ScriptOrder;
use crate::files::units_file::UnitsFile;
use crate::package::Package;
use campfire_values::PackageName;
use campfire_values::ScriptSource;

/// A mode and the packages it depends on, read and parsed, its scripts compiled: what it is on
/// any of its maps, which `ModeRead` reads once and each map's load shares, as an import checks
/// each map it wrote.
#[derive(Debug)]
pub(crate) struct ModeBase {
    pub(crate) mode: Package,
    pub(crate) manifest: ModeManifest,
    /// The script API its scripts were read against, which the load checks them by.
    pub(crate) api: ScriptApi,
    /// The scripts it runs, in order, and which is its own.
    pub(crate) list: ScriptOrder,
    /// The bots it names, by name.
    pub(crate) bots: BotList,
    pub(crate) data: ModeData,
    /// The mode's actions, modifiers, unit types and item types.
    pub(crate) content: PackageContent,
    /// In the order of their names in the mode's manifest.
    pub(crate) dependencies: Vec<Dependent>,
    /// The hooks each script defines, as the load read them, in the order a match compiles them.
    pub(crate) scripts: ScriptBook,
    /// What each script uses of the data, in the same order.
    pub(crate) uses: ScriptUses,
    pub(crate) overrides: Vec<DataOverride>,
}

impl ModeBase {
    /// The mode of `files`, whose manifest is `manifest`, with each package its manifest names in
    /// `dependencies`, in the order of their names: its data read and laid, its unit types read,
    /// its scripts compiled.
    pub(crate) fn read(
        files: &PackageFiles,
        manifest: ModeManifest,
        dependencies: &[(PackageName, &PackageFiles)],
    ) -> Result<Self, LoadError> {
        let mut parser = ScriptHost::new(manifest.script_limits.per_call);
        let api = Release::bind_script_api(&mut parser);
        let name = manifest.header.name.clone();
        let fail = |problem| LoadError::of(&name, problem);
        if PackageIndex::dependency(dependencies.len()).is_none() {
            return Err(fail(LoadProblem::TooManyPackages));
        }
        let mode_data = ModeFile::path();
        let own: TomlTable = files
            .read_data(&mode_data)
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let units_data = UnitsFile::path();
        let units: UnitsFile = files
            .read_data(&units_data)
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let mode = Package::read(files, &manifest.header, &parser, &api)?;
        let mut opened: Vec<OpenedDependent<'_>> = dependencies
            .iter()
            .map(|(name, files)| Dependent::open(name, files))
            .collect::<Result<_, _>>()?;
        Self::check_needs(&manifest, &opened).map_err(fail)?;
        let mut overrides = Vec::new();
        let laid = Self::lay_data(own, &manifest.header.name, &mut opened, &mut overrides);
        let ModeFile {
            scripts: list,
            bots,
            defaults,
            data,
            mut content,
        } = laid
            .read()
            .map_err(|error| ContentError::Data {
                path: mode_data,
                error,
            })
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        content.units = units
            .read(&defaults)
            .map_err(|error| ContentError::Data {
                path: units_data,
                error,
            })
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let dependencies: Vec<Dependent> = opened
            .into_iter()
            .map(|opened| opened.read(&parser, &api, &defaults))
            .collect::<Result<_, _>>()?;
        let bot_scripts = bots.iter().map(|(_, bot)| &bot.script);
        for name in list
            .iter()
            .chain(bot_scripts)
            .filter_map(ScriptSource::package)
        {
            let rules = dependencies.iter().any(|dependent| {
                dependent.package.header.name == *name
                    && matches!(dependent.kind, DependentKind::Rules)
            });
            if !rules {
                return Err(fail(LoadProblem::ScriptPackage(name.clone())));
            }
        }
        let dependents = dependencies.iter().map(|dependent| &dependent.package);
        let scripts = Self::read_hooks(iter::once(&mode).chain(dependents.clone()), &api);
        let uses = Self::read_uses(iter::once(&mode).chain(dependents));
        Ok(Self {
            mode,
            manifest,
            api,
            list,
            bots,
            data,
            content,
            dependencies,
            scripts,
            uses,
            overrides,
        })
    }

    /// Refuses a mode of `manifest` that lacks what a rules package of `dependencies` needs: a
    /// capability, or tick rates within its own (03-game-scripting.md, A rules package states
    /// what it needs).
    fn check_needs(
        manifest: &ModeManifest,
        dependencies: &[OpenedDependent<'_>],
    ) -> Result<(), LoadProblem> {
        for dependent in dependencies {
            let Some(needs) = dependent.needs() else {
                continue;
            };
            let package = dependent.name();
            let lacks = |capability: &&Capability| !manifest.capabilities.contains(**capability);
            if let Some(&capability) = needs.capabilities.iter().find(lacks) {
                return Err(LoadProblem::RulesCapability {
                    package: package.clone(),
                    capability,
                });
            }
            if needs
                .tick_hz
                .is_some_and(|bounds| !bounds.holds(manifest.tick_hz))
            {
                return Err(LoadProblem::RulesRate(package.clone()));
            }
        }
        Ok(())
    }

    /// The mode's data `own`, of the mode `mode`, laid over the data of each rules package of
    /// `dependencies` in their order, each over the ones before it, as the last loaded wins
    /// (03-game-scripting.md, Overrides by load order); each record more than one package gives
    /// goes to `overrides`, with the package whose value stays. The laid data reads as the mode's
    /// `data/mode.toml`, so a field of a rules package's data that does not read fails as that
    /// file.
    fn lay_data(
        own: TomlTable,
        mode: &PackageName,
        dependencies: &mut [OpenedDependent<'_>],
        overrides: &mut Vec<DataOverride>,
    ) -> TomlTable {
        let rules = dependencies.iter_mut().filter_map(|dependent| {
            let data = dependent.take_rules_data()?;
            Some((dependent.name(), data))
        });
        let layers = rules.chain(iter::once((mode, own)));
        let mut laid: Option<TomlTable> = None;
        for (by, mut table) in layers {
            if let Some(below) = laid.take() {
                let records = table.lay_over(below);
                overrides.extend(records.into_iter().map(|record| DataOverride {
                    record,
                    by: by.clone(),
                }));
            }
            laid = Some(table);
        }
        laid.expect("the mode's own data lies last")
    }

    /// The hooks of `api` each script of `packages` defines, in the order a match compiles them:
    /// the mode's scripts, then each dependency's, each package's in the order of their paths.
    fn read_hooks<'p>(
        packages: impl IntoIterator<Item = &'p Package>,
        api: &ScriptApi,
    ) -> ScriptBook {
        let mut book = ScriptBook::new(api.hooks().iter().map(|status| status.hook));
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
}
