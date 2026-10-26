use fluent_syntax::ast::{Entry, Expression, InlineExpression, PatternElement};
use fluent_syntax::parser;

use campfire_actions::SlotKindData;
use campfire_math::Num;
use campfire_navigation::ModeNavigationData;
use campfire_stats::StatOp;
use campfire_values::{Number, Stat};

use super::*;
use crate::zero_hour::combat_import::StageModifier;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::rules_names::RulesNames;

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
    let money = MoneyChoices {
        values: vec![100],
        default: 100,
    };
    let data = |combat: &CombatImport| {
        mode_data(ModeImport {
            start: &start,
            production: &ProductionImport::default(),
            combat,
            upgrades: &UpgradeImport::default(),
            start_money: &money,
        })
    };
    // An empty table would write `actions = {}` into `[params]`, and `armors = {}` under a
    // second `[combat]`.
    let none = data(&CombatImport::default());
    assert!(Toml::parse::<TomlTable>(&none).is_ok(), "{none}");
    assert!(
        !none.contains("actions") && !none.contains("armors") && !none.contains("modifiers"),
        "{none}"
    );
    // A stage's modifier reads back as the modifier that adds to the rate of fire and gives its
    // tag; a fall's, of no stat, as the one that gives its tag alone.
    let names = RulesNames::default();
    let rate = names.rate_of_fire.clone();
    let one = Number::Value(Scalar::Decimal(Num::ONE));
    let stage = StageModifier {
        stats: BTreeMap::from([(rate.clone(), one.clone())]),
        tags: vec![names.continuous_fire[0].clone()],
    };
    let fall = StageModifier {
        stats: BTreeMap::new(),
        tags: vec![names.continuous_fire_slow.clone()],
    };
    let mut combat = CombatImport::default();
    combat.modifiers.insert(name("weapon_w_mean"), stage);
    combat.modifiers.insert(name("weapon_w_slow"), fall);
    let mut written: TomlTable = Toml::parse(&data(&combat)).unwrap();
    let modifiers: ModifiersData = written.take("modifiers").unwrap().unwrap();
    let mean = &modifiers.0[&name("weapon_w_mean")];
    let change = &mean.stats[&rate];
    assert_eq!((change.op, &change.value), (StatOp::Add, &one));
    assert_eq!(mean.tags, [names.continuous_fire[0].clone()]);
    let slow = &modifiers.0[&name("weapon_w_slow")];
    assert!(slow.stats.is_empty());
    assert_eq!(slow.tags, [names.continuous_fire_slow]);
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
    let kinds: Vec<DeclaredName> = DamageType::ALL.map(DamageType::kind).into();
    let damage_kinds = combat.take::<Vec<DeclaredName>>("damage_kinds");
    assert_eq!(damage_kinds.unwrap().unwrap(), kinds);
    let life = combat.take::<DeclaredName>("life").unwrap().unwrap();
    let resources = rules
        .take::<Vec<DeclaredName>>("resources")
        .unwrap()
        .unwrap();
    // Each name the import writes is one the rules package declares.
    let names = RulesNames::default();
    assert_eq!((&life, resources), (&names.life, vec![names.money.clone()]));
    for stat in [&names.life_stat, &names.rate_of_fire] {
        assert!(stats.holds(stat.name()), "{stat}");
    }
    assert!(pools.holds(names.life.as_str()));
    assert_eq!(names.life_stat, Stat::named(names.life.as_str()).unwrap());
    for tag in [
        &names.counts_for_victory,
        &names.structure,
        &names.wall,
        &names.base_defense,
        &names.infantry,
        &names.armor_upgrade,
        &names.weapon_upgrade,
        &names.continuous_fire[0],
        &names.continuous_fire[1],
        &names.continuous_fire_slow,
    ] {
        assert!(tags.holds(tag.as_str()), "{tag}");
    }
    assert_eq!(
        navigation.spawn_tags,
        [names.start.clone(), names.rally.clone()]
    );
    let marker_params = rules.take::<Vec<DeclaredName>>("marker_params");
    assert_eq!(
        marker_params.unwrap().unwrap(),
        [names.waypoint_name.clone(), names.player_slot.clone()]
    );
    let slots = rules.take::<Vec<SlotKindData>>("slots").unwrap().unwrap();
    let slots: Vec<&DeclaredName> = slots.iter().map(|slot| &slot.name).collect();
    assert_eq!(slots, [&names.weapon_slot, &names.command_slot]);
    let slot = turret.take::<DeclaredName>("slot").unwrap().unwrap();
    assert_eq!(slot, names.weapon_slot);
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
            "mode-name = Zero Hour\nchoice-faction = Faction\nchoice-start = Start\n{teams}team-neutral = Neutral\n\
                 action-train-a = A & {{\"{{\"}}B{{\"}}\"}}\n"
        )
    );
}
