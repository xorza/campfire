use std::collections::BTreeMap;

use campfire_package_format::PackageFiles;
use campfire_values::PackageName;

use crate::error::load_problem::LoadProblem;
use crate::error::locale_problem::LocaleProblem;
use crate::error::{LoadError, PackageRef};
use crate::files::manifest::Manifest;
use crate::language::Language;
use crate::locale_file::LocaleFile;

/// A locale package: its name, and its translations of the packages it depends on, by each
/// package's name, then by language.
#[derive(Debug)]
pub struct LocalePackage {
    pub name: PackageName,
    translations: BTreeMap<PackageName, BTreeMap<Language, LocaleFile>>,
}

impl LocalePackage {
    /// The locale package of `files`: each file under `locale/` is `<package>/<language>.ftl`
    /// of a package it depends on, and reads.
    pub fn read(files: &PackageFiles) -> Result<Self, LoadError> {
        let fail = LoadError::new;
        let unnamed = || PackageRef::PackageFingerprint(files.fingerprint());
        let Manifest::Locale(manifest) = Manifest::read(files, &unnamed())? else {
            return Err(fail(unnamed(), LoadProblem::WrongKind));
        };
        let name = manifest.header.name;
        let named = || PackageRef::Name(name.clone());
        let mut translations: BTreeMap<PackageName, BTreeMap<Language, LocaleFile>> =
            BTreeMap::new();
        for path in files.files_under(Language::DIR) {
            let named_file = path.as_str()[Language::DIR.len() + 1..]
                .split_once('/')
                .and_then(|(package, file)| {
                    let language = Language::parse(file.strip_suffix(".ftl")?)?;
                    let (package, _) = manifest.dependencies.get_key_value(package)?;
                    Some((package, language))
                });
            let Some((package, language)) = named_file else {
                let problem = LoadProblem::Locale {
                    path: path.clone(),
                    problem: LocaleProblem::FileName,
                };
                return Err(fail(named(), problem));
            };
            let file = LocaleFile::read(files, path).map_err(|problem| fail(named(), problem))?;
            translations
                .entry(package.clone())
                .or_default()
                .insert(language, file);
        }
        Ok(Self { name, translations })
    }

    /// Its translations of the package `name`, by language.
    pub(crate) fn of(&self, name: &PackageName) -> impl Iterator<Item = (&Language, &LocaleFile)> {
        self.translations.get(name).into_iter().flatten()
    }
}
