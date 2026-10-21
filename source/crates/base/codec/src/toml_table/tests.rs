use std::collections::BTreeMap;

use super::*;
use crate::toml::Toml;

#[test]
fn a_table_fills_only_the_tables_it_holds_with_the_entries_it_lacks() {
    let mut unit: TomlTable =
        Toml::parse("combat = { armor = \"plate\" }\nturret = {}\ntags = [\"a\"]").unwrap();
    let base: TomlTable = Toml::parse(
        "combat = { armor = \"cloth\", on_death = \"stay\" }\nturret = { tolerance_deg = 5 \
             }\nvision = { sight_range = \"6\" }\ntags = { x = 1 }",
    )
    .unwrap();
    unit.fill_held(&base);
    // Its own `armor` stays and `on_death` joins; the empty `turret` takes its default; it
    // gives no `vision`, which stays absent; `tags` is no table, which stays as it was.
    let expected: TomlTable = Toml::parse(
        "combat = { armor = \"plate\", on_death = \"stay\" }\nturret = { tolerance_deg = 5 \
             }\ntags = [\"a\"]",
    )
    .unwrap();
    assert_eq!(unit, expected);
}

#[test]
fn a_table_gives_its_keys_out_and_reads_the_rest_as_a_type() {
    let mut table: TomlTable = Toml::parse("cell = 10\nstep = 5\nname = \"grid\"").unwrap();
    assert!(table.holds("step") && !table.holds("none"));
    assert_eq!(table.keys().collect::<Vec<_>>(), ["cell", "name", "step"]);
    assert_eq!(table.take::<String>("name").unwrap().unwrap(), "grid");
    assert!(table.take::<String>("name").is_none());
    assert!(table.clone().take_value("step").is_some());
    assert!(table.take_value("name").is_none());
    assert!(table.take::<String>("cell").unwrap().is_err());
    // `cell` went with its failed read; `step` and the absent `none` split off.
    let kept: Vec<_> = table.clone().into_entries().collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].0, "step");
    assert_eq!(kept[0].1.clone().read::<i64>().unwrap(), 5);
    let split = table.take_keys(["step", "none"]);
    assert_eq!(
        TomlValue::from(split.clone()).read::<BTreeMap<String, i64>>(),
        split.clone().read::<BTreeMap<String, i64>>()
    );
    assert_eq!(
        split.read::<BTreeMap<String, i64>>().unwrap(),
        BTreeMap::from([("step".to_owned(), 5)])
    );
    assert_eq!(
        table.read::<BTreeMap<String, i64>>().unwrap(),
        BTreeMap::new()
    );
}

#[test]
fn a_table_laid_over_another_keeps_its_own_values_and_merges_its_tables_keys() {
    let mut mode: TomlTable =
        Toml::parse("name = \"mode\"\n[combat]\nlife = \"hp\"\n[state]\nphase = 1\nround = 2")
            .unwrap();
    let rules: TomlTable = Toml::parse(
            "name = \"rules\"\ntimers = [\"a\"]\n[combat]\nkinds = [\"x\"]\n[state]\nphase = 9\nscore = 3",
        )
        .unwrap();
    // `name` stays the mode's, `timers` joins it, and the tables' keys merge: `[combat]` takes
    // the rules' `kinds` beside its own `life`, and the state the rules' `score`, its own
    // `phase` over the rules'.
    let overridden = mode.lay_over(rules);
    assert_eq!(overridden, ["name", "state.phase"]);
    assert_eq!(
        mode.clone().take::<String>("name").unwrap().unwrap(),
        "mode"
    );
    let combat: BTreeMap<String, toml::Value> = mode.clone().take("combat").unwrap().unwrap();
    assert_eq!(combat.keys().collect::<Vec<_>>(), ["kinds", "life"]);
    assert_eq!(combat["life"].as_str(), Some("hp"));
    let state: BTreeMap<String, i64> = mode.clone().take("state").unwrap().unwrap();
    let expected = [("phase", 1), ("round", 2), ("score", 3)];
    assert_eq!(
        state,
        expected.map(|(key, value)| (key.to_owned(), value)).into()
    );
    assert_eq!(mode.take::<Vec<String>>("timers").unwrap().unwrap(), ["a"]);
    // A key that is a table on one side only stays the top's whole, and a table two levels
    // down is one key's value, whole.
    let mut flat: TomlTable = Toml::parse("state = 1").unwrap();
    let tabled: TomlTable = Toml::parse("[state]\nphase = 1").unwrap();
    assert_eq!(flat.lay_over(tabled), ["state"]);
    assert_eq!(flat.take::<i64>("state").unwrap().unwrap(), 1);
    let mut deep: TomlTable = Toml::parse("[combat.armors.tank]\nshell = 1").unwrap();
    let under: TomlTable = Toml::parse("[combat.armors.jeep]\nshell = 2").unwrap();
    assert_eq!(deep.lay_over(under), ["combat.armors"]);
    let combat: BTreeMap<String, toml::Value> = deep.take("combat").unwrap().unwrap();
    let armors = combat["armors"].as_table().unwrap();
    assert_eq!(armors.keys().collect::<Vec<_>>(), ["tank"]);
}
