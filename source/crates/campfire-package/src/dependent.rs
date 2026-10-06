use campfire_capabilities::{PackageContent, ScriptApi};
use campfire_script::ScriptHost;

use crate::avatar_unit::AvatarUnit;
use crate::error::LoadError;
use crate::error::load_problem::LoadProblem;
use crate::files::avatar_data::AvatarData;
use crate::files::manifest::Manifest;
use crate::package::Package;
use crate::package_dir::PackageDir;
use crate::package_files::PackageFiles;

const AVATAR_DATA: &str = "data/avatar.toml";
const LOADOUT_DATA: &str = "data/loadout.toml";

/// A package the mode depends on: it, its content, and its kind.
#[derive(Debug)]
pub struct Dependent {
    pub package: Package,
    pub content: PackageContent,
    pub kind: DependentKind,
}

/// The kind of a package the mode depends on: an avatar, with its one unit type, or a loadout.
#[derive(Debug)]
pub enum DependentKind {
    Avatar(Box<AvatarUnit>),
    Loadout,
}

impl Dependent {
    /// The package of `files`, which the mode names `name`: an avatar or loadout package of that
    /// name.
    pub(crate) fn read(
        name: &str,
        files: &PackageFiles,
        parser: &ScriptHost,
        api: &ScriptApi,
    ) -> Result<Dependent, LoadError> {
        let fail = |problem| LoadError::of(name, problem);
        let manifest: Manifest = files
            .read_data(&PackageDir::engine_path(PackageDir::MANIFEST))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let header = manifest.header();
        if header.name != name {
            return Err(fail(LoadProblem::OtherName(header.name.clone())));
        }
        let (content, kind) = match &manifest {
            Manifest::Avatar(_) => {
                let AvatarData {
                    name,
                    unit,
                    content,
                } = files
                    .read_data(&PackageDir::engine_path(AVATAR_DATA))
                    .map_err(LoadProblem::Content)
                    .map_err(fail)?;
                let avatar = AvatarUnit { name, unit };
                (content, DependentKind::Avatar(Box::new(avatar)))
            }
            Manifest::Loadout(_) => {
                let content = files.read_data(&PackageDir::engine_path(LOADOUT_DATA));
                let content = content.map_err(LoadProblem::Content).map_err(fail)?;
                (content, DependentKind::Loadout)
            }
            Manifest::Mode(_) | Manifest::Locale(_) => return Err(fail(LoadProblem::WrongKind)),
        };
        let package = Package::read(files, header, parser, api)?;
        Ok(Dependent {
            package,
            content,
            kind,
        })
    }
}
