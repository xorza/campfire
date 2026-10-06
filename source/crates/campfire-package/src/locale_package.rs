use std::collections::BTreeMap;

use campfire_capabilities::{ApiVersion, PackagePath};

use crate::error::load_problem::LoadProblem;
use crate::error::locale_problem::LocaleProblem;
use crate::error::{LoadError, PackageRef};
use crate::files::manifest::Manifest;
use crate::language::Language;
use crate::locale_file::LocaleFile;
use crate::package_dir::PackageDir;
use crate::package_files::PackageFiles;
use crate::package_text::LOCALE;

/// A locale package: its name, and its translations of the packages it depends on, by each
/// package's name, then by language.
#[derive(Debug)]
pub struct LocalePackage {
    pub name: String,
    translations: BTreeMap<String, BTreeMap<Language, LocaleFile>>,
}

impl LocalePackage {
    /// The locale package of `files`: each file under `locale/` is `<package>/<language>.ftl`
    /// of a package it depends on, and reads.
    pub fn read(files: &PackageFiles) -> Result<LocalePackage, LoadError> {
        let fail = LoadError::new;
        let manifest_path = PackagePath::parse(PackageDir::MANIFEST).expect("a package path");
        let unnamed = || PackageRef::Fingerprint(files.fingerprint());
        let manifest = files
            .read_data(&manifest_path)
            .map_err(|error| fail(unnamed(), LoadProblem::Content(error)))?;
        let Manifest::Locale(manifest) = manifest else {
            return Err(fail(unnamed(), LoadProblem::WrongKind));
        };
        let name = manifest.header.name;
        let named = || PackageRef::Name(name.clone());
        if !ApiVersion::RELEASE.loads(manifest.header.api) {
            return Err(fail(named(), LoadProblem::OtherApi(manifest.header.api)));
        }
        let mut translations: BTreeMap<String, BTreeMap<Language, LocaleFile>> = BTreeMap::new();
        for path in files.files_under(LOCALE) {
            let named_file =
                path.as_str()[LOCALE.len() + 1..]
                    .split_once('/')
                    .and_then(|(package, file)| {
                        let language = Language::parse(file.strip_suffix(".ftl")?)?;
                        manifest
                            .dependencies
                            .contains_key(package)
                            .then_some((package, language))
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
                .entry(package.to_owned())
                .or_default()
                .insert(language, file);
        }
        Ok(LocalePackage { name, translations })
    }

    /// Its translations of the package `package`, by language.
    pub(crate) fn of(&self, package: &str) -> impl Iterator<Item = (&Language, &LocaleFile)> {
        self.translations.get(package).into_iter().flatten()
    }
}
