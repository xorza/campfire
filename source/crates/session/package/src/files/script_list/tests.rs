use std::fmt::Write;

use campfire_codec::Toml;

use super::*;

#[derive(Debug, Deserialize)]
struct File {
    scripts: ScriptOrder,
}

fn read(text: &str) -> Result<ScriptOrder, String> {
    Toml::parse::<File>(text)
        .map(|file| file.scripts)
        .map_err(|error| error.to_string())
}

fn own(path: &str) -> ScriptSource {
    ScriptSource::Own(PackagePath::parse(path).unwrap())
}

#[test]
fn a_list_names_its_scripts_in_order_and_marks_the_mode_s_own() {
    // The only entry is the mode's own, marked or not.
    let one = read("scripts = [\"scripts/mode.rhai\"]").unwrap();
    assert_eq!(
        one.iter().cloned().collect::<Vec<_>>(),
        [own("scripts/mode.rhai")]
    );
    assert_eq!(one.mode(), ListPlace::FIRST);
    let marked = read("scripts = [{ path = \"scripts/mode.rhai\", mode = true }]").unwrap();
    assert_eq!(marked, one);
    // In a list of more, the marked one, here the second: a rules package's script.
    let list = read(
        "scripts = [\"scripts/first.rhai\", { package = \"test-rules\", path = \
         \"scripts/mode.rhai\", mode = true }, { path = \"scripts/last.rhai\" }]",
    )
    .unwrap();
    let rules = ScriptSource::Rules {
        package: PackageName::new("test-rules").unwrap(),
        path: PackagePath::parse("scripts/mode.rhai").unwrap(),
    };
    let held: Vec<_> = list.iter().cloned().collect();
    assert_eq!(
        held,
        [own("scripts/first.rhai"), rules, own("scripts/last.rhai")]
    );
    assert_eq!(list.mode(), ListPlace::new(1).unwrap());
    // Each writes its entries as tables, the mode's marked in a list of more than one, and
    // reads back to itself; a list of one is `one`'s.
    let written = |order: &ScriptOrder| {
        let text = Toml::write(&std::collections::BTreeMap::from([(
            ScriptOrder::KEY,
            order,
        )]));
        read(&text).unwrap()
    };
    assert_eq!(written(&list), list);
    assert_eq!(written(&one), one);
    assert_eq!(ScriptOrder::one(own("scripts/mode.rhai")), one);
    // The same path of two packages is two scripts.
    let two = "scripts = [{ path = \"scripts/mode.rhai\", mode = true }, { package = \
               \"test-rules\", path = \"scripts/mode.rhai\" }]";
    assert_eq!(read(two).unwrap().iter().len(), 2);
}

#[test]
fn a_list_of_no_script_one_twice_or_no_one_mark_fails() {
    let refused = |text: &str, why: &str| {
        let error = read(text).unwrap_err();
        assert!(error.contains(why), "{text}: {error}");
    };
    refused("scripts = []", "lists no script");
    refused(
        "scripts = [\"scripts/a.rhai\", { path = \"scripts/a.rhai\", mode = true }]",
        "lists the script scripts/a.rhai twice",
    );
    refused(
        "scripts = [\"scripts/a.rhai\", \"scripts/b.rhai\"]",
        "marks none of its scripts",
    );
    refused(
        "scripts = [{ path = \"scripts/a.rhai\", mode = true }, { path = \"scripts/b.rhai\", \
         mode = true }]",
        "marks more than one",
    );
    let mut many = String::from("scripts = [{ path = \"scripts/0.rhai\", mode = true }");
    for at in 1..=ListPlace::LIMIT {
        write!(many, ", \"scripts/{at}.rhai\"").unwrap();
    }
    many.push(']');
    refused(&many, "at most 256 scripts");
    // An entry with a key it does not take.
    assert!(read("scripts = [{ path = \"scripts/a.rhai\", ends = true }]").is_err());
}
