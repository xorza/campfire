use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentError, FluentResource};

use crate::error::LoadError;
use crate::error::load_problem::LoadProblem;
use crate::error::locale_problem::LocaleProblem;
use crate::language::Language;
use crate::locale_package::LocalePackage;
use crate::message_id::MessageId;
use crate::mode_packages::ModePackages;

type Bundle = FluentBundle<Arc<FluentResource>>;

/// The human text of a mode's packages, in each language they and the locale packages give:
/// what a client shows. The sim never reads it.
pub struct Texts {
    /// By package name.
    packages: BTreeMap<String, PackageBundles>,
}

/// A package's messages in each language: its own file of the language first, then the locale
/// packages' translations, in the order of their names.
struct PackageBundles {
    own: Language,
    bundles: BTreeMap<Language, Bundle>,
}

impl Texts {
    /// The text of `packages`, with the translations `locales` give them; an error when a
    /// translation defines a message its package's own language does not.
    pub fn new(packages: &ModePackages, locales: &[LocalePackage]) -> Result<Texts, LoadError> {
        let mut locales: Vec<&LocalePackage> = locales.iter().collect();
        locales.sort_by(|a, b| a.name.cmp(&b.name));
        let mut texts = Texts {
            packages: BTreeMap::new(),
        };
        for view in packages.packages() {
            let package = view.package;
            let text = &package.text;
            let mut files: BTreeMap<&Language, Vec<Arc<FluentResource>>> = BTreeMap::new();
            for (language, file) in &text.files {
                files
                    .entry(language)
                    .or_default()
                    .push(Arc::clone(&file.resource));
            }
            for locale in &locales {
                for (language, file) in locale.of(&package.header.name) {
                    if let Some(id) = file.stray(text.own()) {
                        let problem = LoadProblem::Locale {
                            path: file.path.clone(),
                            problem: LocaleProblem::Stray(id.clone()),
                        };
                        return Err(LoadError::of(&locale.name, problem));
                    }
                    files
                        .entry(language)
                        .or_default()
                        .push(Arc::clone(&file.resource));
                }
            }
            let bundles = files
                .into_iter()
                .map(|(language, resources)| (language.clone(), bundle(language, resources)))
                .collect();
            let bundles = PackageBundles {
                own: text.language.clone(),
                bundles,
            };
            texts.packages.insert(package.header.name.clone(), bundles);
        }
        Ok(texts)
    }

    /// The text of message `id` of package `package` in `language`, or else in the package's
    /// own language; `None` when neither gives the message a value that formats.
    pub fn text(&self, package: &str, id: &MessageId, language: &Language) -> Option<String> {
        let package = self.packages.get(package)?;
        [language, &package.own].into_iter().find_map(|language| {
            let bundle = package.bundles.get(language)?;
            let pattern = bundle.get_message(id.as_str())?.value()?;
            let mut errors = Vec::new();
            let text = bundle.format_pattern(pattern, None, &mut errors);
            errors.is_empty().then(|| text.into_owned())
        })
    }
}

/// The bundle of `language` of `resources`, in order: an earlier one's message wins, as a
/// package's own file comes before the translations.
fn bundle(language: &Language, resources: Vec<Arc<FluentResource>>) -> Bundle {
    let mut bundle = Bundle::new_concurrent(vec![language.identifier()]);
    for resource in resources {
        if let Err(errors) = bundle.add_resource(resource) {
            debug_assert!(
                errors
                    .iter()
                    .all(|error| matches!(error, FluentError::Overriding { .. })),
                "a checked file only overrides: {errors:?}"
            );
        }
    }
    bundle
}

impl fmt::Debug for Texts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(&self.packages).finish()
    }
}

impl fmt::Debug for PackageBundles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PackageBundles")
            .field("own", &self.own)
            .field("languages", &self.bundles.keys().collect::<Vec<_>>())
            .finish()
    }
}
