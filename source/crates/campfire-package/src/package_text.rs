use std::collections::BTreeMap;

use crate::error::load_problem::LoadProblem;
use crate::error::locale_problem::LocaleProblem;
use crate::language::Language;
use crate::locale_file::LocaleFile;
use crate::package_files::PackageFiles;

/// Where a package holds its human text.
pub(crate) const LOCALE: &str = "locale";

/// A package's human text: its own language, and its `locale/<language>.ftl` files by language.
#[derive(Debug)]
pub struct PackageText {
    pub(crate) language: Language,
    pub(crate) files: BTreeMap<Language, LocaleFile>,
}

impl PackageText {
    /// The text of `files`, a package of `language`: each file under `locale/` is
    /// `<language>.ftl` and reads, and another language's file defines only messages the own
    /// file defines.
    pub(crate) fn read(
        files: &PackageFiles,
        language: &Language,
    ) -> Result<PackageText, LoadProblem> {
        let mut text = PackageText {
            language: language.clone(),
            files: BTreeMap::new(),
        };
        for path in files.files_under(LOCALE) {
            let named = path.as_str()[LOCALE.len() + 1..]
                .strip_suffix(".ftl")
                .and_then(Language::parse);
            let Some(language) = named else {
                return Err(LoadProblem::Locale {
                    path: path.clone(),
                    problem: LocaleProblem::FileName,
                });
            };
            text.files.insert(language, LocaleFile::read(files, path)?);
        }
        let own = text.own();
        for (language, file) in &text.files {
            if *language == text.language {
                continue;
            }
            if let Some(id) = file.stray(own) {
                return Err(LoadProblem::Locale {
                    path: file.path.clone(),
                    problem: LocaleProblem::Stray(id.clone()),
                });
            }
        }
        Ok(text)
    }

    /// The file of its own language, if it has one.
    pub(crate) fn own(&self) -> Option<&LocaleFile> {
        self.files.get(&self.language)
    }

    /// Whether its own language's file gives message `id` a value.
    pub(crate) fn gives(&self, id: &str) -> bool {
        self.own().is_some_and(|file| file.gives(id))
    }
}
