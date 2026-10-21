use fluent_syntax::ast::{Entry, Expression, InlineExpression, PatternElement};
use fluent_syntax::parser;

use campfire_actions::SlotKindData;
use campfire_math::Num;
use campfire_navigation::ModeNavigationData;

use super::*;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::imported_type::{INFANTRY, LIFE, MONEY, STRUCTURE, VICTORY, WALL};
use crate::zero_hour::map_import::START;

#[test]
fn a_mode_of_no_weapon_armor_or_action_writes_none_of_their_tables() {
    let name = |name| DeclaredName::new(name).unwrap();
    let start = SkirmishStart {
        factions: vec![FactionStart {
            name: name("america"),
            building: name("center"),
            units: vec![name("dozer")],
            offset: Num::int(2),
        }],
    };
    let data = mode_data(ModeImport {
        start: &start,
        production: &ProductionImport::default(),
        combat: &CombatImport::default(),
        start_money: &MoneyChoices {
            values: vec![100],
            default: 100,
        },
    });
    // An empty table would write `actions = {}` into `[params]`, and `armors = {}` under a
    // second `[combat]`.
    assert!(Toml::parse::<TomlTable>(&data).is_ok(), "{data}");
    assert!(
        !data.contains("actions") && !data.contains("armors"),
        "{data}"
    );
}

#[test]
fn the_rules_package_s_data_declares_each_name_the_import_gives_its_types() {
    // The rules data, as the import ships it, holds the game's damage types in the order of
    // `DamageType`, the life pool and stats, the tags and slot kinds the imported types name,
    // the command buttons' `command` among them, the player resource and the markers' start
    // tag, and the slot kind its types' turrets turn by default.
    let (_, text) = RULES_FILES
        .into_iter()
        .find(|(path, _)| *path == "data/rules.toml")
        .unwrap();
    let mut rules: TomlTable = Toml::parse(std::str::from_utf8(text).unwrap()).unwrap();
    let mut take = |key: &str| rules.take::<TomlTable>(key).unwrap().unwrap();
    let mut combat = take("combat");
    let (stats, pools, tags) = (take("stats"), take("pools"), take("tags"));
    let navigation: ModeNavigationData = take("navigation").read().unwrap();
    let mut turret = take("defaults")
        .take::<TomlTable>("turret")
        .unwrap()
        .unwrap();
    let declared = |name: &str| DeclaredName::new(name).unwrap();
    let kinds: Vec<DeclaredName> = DamageType::ALL.map(DamageType::kind).into();
    let damage_kinds = combat.take::<Vec<DeclaredName>>("damage_kinds");
    assert_eq!(damage_kinds.unwrap().unwrap(), kinds);
    let life = combat.take::<DeclaredName>("life").unwrap().unwrap();
    let resources = rules
        .take::<Vec<DeclaredName>>("resources")
        .unwrap()
        .unwrap();
    assert_eq!((life, resources), (declared(LIFE), vec![declared(MONEY)]));
    assert!(stats.holds(LIFE) && pools.holds(LIFE));
    for tag in [VICTORY, STRUCTURE, WALL, INFANTRY] {
        assert!(tags.holds(tag), "{tag}");
    }
    assert_eq!(navigation.spawn_tags, [declared(START)]);
    let slots = rules.take::<Vec<SlotKindData>>("slots").unwrap().unwrap();
    let slots: Vec<&str> = slots.iter().map(|slot| slot.name.as_str()).collect();
    assert_eq!(slots, ["weapon", "command"]);
    let slot = turret.take::<DeclaredName>("slot").unwrap().unwrap();
    assert_eq!(slot, declared("weapon"));
}

/// The text the Fluent message `value` gives, as `fluent-syntax` parses it.
fn read_back(value: &str) -> String {
    let source = format!("key = {value}\n");
    let resource = parser::parse(source.as_str()).expect("a message parses");
    let [Entry::Message(message)] = resource.body.as_slice() else {
        panic!("one message: {source:?}");
    };
    let pattern = message.value.as_ref().expect("a message with a value");
    pattern
        .elements
        .iter()
        .map(|element| match element {
            PatternElement::TextElement { value } => *value,
            PatternElement::Placeable {
                expression: Expression::Inline(InlineExpression::StringLiteral { value }),
            } => *value,
            PatternElement::Placeable { .. } => panic!("a literal: {source:?}"),
        })
        .collect()
}

#[test]
fn a_name_is_a_fluent_value_that_gives_it_back() {
    // Plain text is itself; braces are string literals; a later line is indented, its
    // leading `[`, `*` or `.` a literal too, where the first line's is plain; and the empty
    // text an empty literal.
    assert_eq!(fluent_value("Dozer"), "Dozer");
    assert_eq!(fluent_value("a {b}"), r#"a {"{"}b{"}"}"#);
    assert_eq!(
        fluent_value("[x]\n[y]\n*z\n.w"),
        "[x]\n    {\"[\"}y]\n    {\"*\"}z\n    {\".\"}w"
    );
    assert_eq!(fluent_value(""), r#"{""}"#);
    // Fluent's own parser reads each back as it was, the spaces that start or end a line and
    // the empty lines among them.
    for text in [
        "Dozer",
        "",
        "a {b}",
        "[x]\n[y]\n*z\n.w",
        "  lead",
        "trail  ",
        "a\n  b",
        "a\n\nb",
        "a\n",
        "\nb",
        "tab\tx",
        "}{",
        "   ",
        " a \n b \n",
        "a\n .b",
    ] {
        assert_eq!(read_back(&fluent_value(text)), text, "{text:?}");
    }
    // A mode's text holds its names, each of its eight players' teams among them, then
    // each action's.
    let texts = [("action-train-a".to_owned(), "A & {B}".to_owned())].into();
    let mut teams = String::new();
    for player in 1..=8 {
        writeln!(teams, "team-player-{player} = Player {player}").unwrap();
    }
    assert_eq!(
        mode_text(&texts),
        format!(
            "mode-name = Zero Hour\nchoice-faction = Faction\n{teams}team-neutral = Neutral\n\
                 action-train-a = A & {{\"{{\"}}B{{\"}}\"}}\n"
        )
    );
}
