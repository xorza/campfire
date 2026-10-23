use std::mem;

use campfire_capabilities::{PackageContent, Release};
use campfire_codec::TomlTable;
use campfire_declarations::UnitDefaults;
use campfire_script::{ScriptApi, ScriptHost};

use crate::avatar_unit::AvatarUnit;
use crate::error::load_problem::LoadProblem;
use crate::error::{LoadError, PackageRef};
use crate::files::avatar_file::{AvatarFile, AvatarSeed};
use crate::files::bot_list::BotList;
use crate::files::loadout_file::{LoadoutFile, LoadoutSeed};
use crate::files::manifest::Manifest;
use crate::files::rules_manifest::RulesNeeds;
use crate::files::script_list::ScriptOrder;
use crate::package::Package;
use crate::package_name::PackageName;
use campfire_package_format::ContentError;
use campfire_package_format::PackageFiles;

const AVATAR_DATA: &str = "data/avatar.toml";
const LOADOUT_DATA: &str = "data/loadout.toml";
const RULES_DATA: &str = "data/rules.toml";

/// A package the mode depends on: it, its content, and its kind.
#[derive(Debug)]
pub struct Dependent {
    pub package: Package,
    pub content: PackageContent,
    pub kind: DependentKind,
}

/// The kind of a package the mode depends on: an avatar, with its one unit type, a loadout, or a
/// rules package, whose data the mode's lies over as it loads.
#[derive(Debug)]
pub enum DependentKind {
    Avatar(Box<AvatarUnit>),
    Loadout,
    Rules,
}

impl Dependent {
    /// The package of `files`, which the mode names `name`, read up to its data: an avatar,
    /// loadout or rules package of that name.
    pub(crate) fn open<'f>(
        name: &'f PackageName,
        files: &'f PackageFiles,
    ) -> Result<OpenedDependent<'f>, LoadError> {
        let fail = |problem| LoadError::of(name, problem);
        let manifest = Manifest::read(files, &PackageRef::Name(name.clone()))?;
        let header = manifest.header();
        if header.name != *name {
            return Err(fail(LoadProblem::OtherName(header.name.clone())));
        }
        let read = |path| files.read_data::<TomlTable>(&PackageFiles::engine_path(path));
        let (kind, data) = match &manifest {
            Manifest::Avatar(_) => {
                let data = read(AVATAR_DATA)
                    .map_err(LoadProblem::Content)
                    .map_err(fail)?;
                (FileKind::Avatar, data)
            }
            Manifest::Loadout(_) => {
                let data = read(LOADOUT_DATA)
                    .map_err(LoadProblem::Content)
                    .map_err(fail)?;
                (FileKind::Loadout, data)
            }
            Manifest::Rules(rules) => {
                let data = match read(RULES_DATA) {
                    Ok(data) => data,
                    Err(ContentError::Missing { .. }) => TomlTable::default(),
                    Err(error) => return Err(fail(LoadProblem::Content(error))),
                };
                let readers = Release::sections();
                let content_keys = PackageContent::keys(&readers);
                let own = [ScriptOrder::KEY, BotList::KEY]
                    .into_iter()
                    .chain(content_keys);
                if let Some(key) = own.into_iter().find(|&key| data.holds(key)) {
                    return Err(fail(LoadProblem::RulesKey(key.to_owned())));
                }
                (FileKind::Rules(rules.needs.clone()), data)
            }
            Manifest::Mode(_) | Manifest::Locale(_) => return Err(fail(LoadProblem::WrongKind)),
        };
        Ok(OpenedDependent {
            name,
            files,
            manifest,
            kind,
            data,
        })
    }
}

/// A package the mode depends on, read up to its data: its manifest, its kind, and the data file
/// of its kind kept whole, as the mode's defaults lie under its unit types once the mode's data
/// is laid (03-game-scripting.md, Defaults under each unit type).
#[derive(Debug)]
pub(crate) struct OpenedDependent<'f> {
    name: &'f PackageName,
    files: &'f PackageFiles,
    manifest: Manifest,
    kind: FileKind,
    /// An avatar's or a loadout's data file, or a rules package's data, checked, empty without
    /// one.
    data: TomlTable,
}

/// The kind of a package the mode depends on, as its manifest gives it.
#[derive(Debug)]
enum FileKind {
    Avatar,
    Loadout,
    Rules(RulesNeeds),
}

impl OpenedDependent<'_> {
    pub(crate) const fn name(&self) -> &PackageName {
        self.name
    }

    /// What it needs of the mode, a rules package's; none for another kind.
    pub(crate) const fn needs(&self) -> Option<&RulesNeeds> {
        match &self.kind {
            FileKind::Rules(needs) => Some(needs),
            FileKind::Avatar | FileKind::Loadout => None,
        }
    }

    /// Its data, a rules package's, moved out for the mode's own to lie over; none for another
    /// kind.
    pub(crate) fn take_rules_data(&mut self) -> Option<TomlTable> {
        self.needs()?;
        Some(mem::take(&mut self.data))
    }

    /// The package, its data read with `defaults` under its unit types' sections, and its scripts
    /// parsed with `host` against `api`.
    pub(crate) fn read(
        self,
        host: &ScriptHost,
        api: &ScriptApi,
        defaults: &UnitDefaults,
    ) -> Result<Dependent, LoadError> {
        let fail = |problem| LoadError::of(self.name, problem);
        let data = |path| {
            let path = PackageFiles::engine_path(path);
            move |error| fail(LoadProblem::Content(ContentError::Data { path, error }))
        };
        let (content, kind) = match self.kind {
            FileKind::Avatar => {
                let AvatarFile {
                    name,
                    unit,
                    content,
                } = (self.data)
                    .read_with(AvatarSeed(defaults))
                    .map_err(data(AVATAR_DATA))?;
                let avatar = AvatarUnit { name, unit };
                (content, DependentKind::Avatar(Box::new(avatar)))
            }
            FileKind::Loadout => {
                let read = self.data.read_with(LoadoutSeed(defaults));
                let LoadoutFile(content) = read.map_err(data(LOADOUT_DATA))?;
                (content, DependentKind::Loadout)
            }
            FileKind::Rules(_) => (PackageContent::default(), DependentKind::Rules),
        };
        let package = Package::read(self.files, self.manifest.header(), host, api)?;
        Ok(Dependent {
            package,
            content,
            kind,
        })
    }
}
