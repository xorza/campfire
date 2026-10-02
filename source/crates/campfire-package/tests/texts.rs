//! A package's human text, found in a language: the player's from the package or a locale
//! package, else the package's own.

use campfire_package::{
    Language, LoadError, LoadProblem, LocalePackage, LocaleProblem, MessageId, ModePackages,
    PackageRef, Texts,
};

use crate::moba::{Edit, edited, edited_at};

fn language(text: &str) -> Language {
    Language::parse(text).unwrap()
}

/// The German locale package of the reference packages, with `edits` made.
fn german<'a>(
    edits: impl IntoIterator<Item = (&'a str, Edit<'a>)>,
) -> Result<LocalePackage, LoadError> {
    LocalePackage::read(&edited_at("locales/de", edits).read().unwrap())
}

#[test]
fn a_heros_name_reads_in_the_players_language_or_else_in_the_heros_own() {
    let packages = ModePackages::from_package_dir(&edited([])).unwrap();
    let name = MessageId::new("hero-name").unwrap();
    let texts = Texts::new(&packages, &[german([]).unwrap()]).unwrap();
    let text = |package, language_text| texts.text(package, &name, &language(language_text));
    // German from the locale package; English, the hero's own; French, which no package gives,
    // in the hero's own.
    assert_eq!(text("hero-husk", "de").as_deref(), Some("Hülse"));
    assert_eq!(text("hero-rime", "de").as_deref(), Some("Raureif"));
    assert_eq!(text("hero-husk", "en").as_deref(), Some("Husk"));
    assert_eq!(text("hero-husk", "fr").as_deref(), Some("Husk"));
    // No message of that id, and no package of that name, give none.
    let title = MessageId::new("hero-title").unwrap();
    assert_eq!(texts.text("hero-husk", &title, &language("en")), None);
    assert_eq!(text("hero-nobody", "en"), None);

    // Without the locale package, German is the hero's own language.
    let texts = Texts::new(&packages, &[]).unwrap();
    assert_eq!(
        texts.text("hero-husk", &name, &language("de")).as_deref(),
        Some("Husk")
    );

    // The hero's own German file comes before the locale package's.
    let own = (
        "heroes/husk/locale/de.ftl",
        Edit::Create("hero-name = Hüllenwesen\n"),
    );
    let packages = ModePackages::from_package_dir(&edited([own])).unwrap();
    let texts = Texts::new(&packages, &[german([]).unwrap()]).unwrap();
    assert_eq!(
        texts.text("hero-husk", &name, &language("de")).as_deref(),
        Some("Hüllenwesen")
    );
    assert_eq!(
        texts.text("hero-gale", &name, &language("de")).as_deref(),
        Some("Bö")
    );
}

#[test]
fn a_locale_package_translates_only_the_messages_of_the_packages_it_depends_on() {
    let file_name = |error: LoadError, file: &str| {
        assert_eq!(error.package, PackageRef::Name("moba-de".to_owned()));
        assert!(
            matches!(&*error.problem, LoadProblem::Locale { path, problem: LocaleProblem::FileName } if path.as_str() == file),
            "{error}"
        );
    };
    // A package it does not depend on, a file outside a package's directory, and a file name
    // that is no language.
    let lancer = "locales/de/locale/hero-lancer/de.ftl";
    file_name(
        german([(lancer, Edit::Create("hero-name = Lanzer\n"))]).unwrap_err(),
        "locale/hero-lancer/de.ftl",
    );
    let loose = "locales/de/locale/de.ftl";
    file_name(
        german([(loose, Edit::Create("hero-name = Hülse\n"))]).unwrap_err(),
        "locale/de.ftl",
    );
    let english = "locales/de/locale/hero-husk/english.ftl.txt";
    file_name(
        german([(english, Edit::Create("hero-name = Husk\n"))]).unwrap_err(),
        "locale/hero-husk/english.ftl.txt",
    );

    // A translation of a message its package does not define.
    let stray = (
        "locales/de/locale/hero-husk/de.ftl",
        Edit::Replace("hero-name", "hero-nme"),
    );
    let packages = ModePackages::from_package_dir(&edited([])).unwrap();
    let error = Texts::new(&packages, &[german([stray]).unwrap()]).unwrap_err();
    assert_eq!(error.package, PackageRef::Name("moba-de".to_owned()));
    assert!(
        matches!(&*error.problem, LoadProblem::Locale { path, problem: LocaleProblem::Stray(id) } if path.as_str() == "locale/hero-husk/de.ftl" && id.as_str() == "hero-nme"),
        "{error}"
    );

    // A mode is no locale package.
    let mode = LocalePackage::read(&edited([]).read().unwrap()).unwrap_err();
    assert!(matches!(*mode.problem, LoadProblem::WrongKind), "{mode}");
}
