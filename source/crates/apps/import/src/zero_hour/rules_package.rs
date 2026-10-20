use std::collections::BTreeMap;
use std::fmt::Write as _;

use campfire_codec::Toml;
use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::Serialize;

use crate::zero_hour::combat_import::{CombatImport, ImportedWeapon};
use crate::zero_hour::format::ini::multiplayer_ini::MoneyChoices;
use crate::zero_hour::map_import::SLOTS;
use crate::zero_hour::production_import::{ImportedAction, ProductionImport};
use crate::zero_hour::skirmish_start::SkirmishStart;

/// Zero Hour's rules package as the importer ships it, `source/packages/zero-hour/`: each file but
/// its index, by its path, which the import writes beside the imported package.
pub(crate) const RULES_FILES: [(&str, &[u8]); 3] = [
    (
        "manifest.toml",
        include_bytes!("../../../../../packages/zero-hour/manifest.toml"),
    ),
    (
        "data/rules.toml",
        include_bytes!("../../../../../packages/zero-hour/data/rules.toml"),
    ),
    (
        "scripts/mode.rhai",
        include_bytes!("../../../../../packages/zero-hour/scripts/mode.rhai"),
    ),
];

/// The imported package's manifest: a mode of eight players, each on a team of its own, beside the
/// `neutral` team every placed object stands on, whose units drive, are made and fight with shells:
/// of `stats`, `orders`, `navigation`, `abilities`, `production`, `combat` and `projectiles`, no
/// unit faster than `max_move_speed` meters a second, at the game's 30 logic frames a second,
/// depending on the rules package beside it.
pub(crate) fn mode_manifest(max_move_speed: Num) -> String {
    let teams: Vec<String> = (1..=SLOTS)
        .map(|player| format!("{{ name = \"player_{player}\", seats = 1 }}"))
        .chain(["{ name = \"neutral\", seats = 0 }".to_owned()])
        .collect();
    format!(
        r#"# Zero Hour, imported from the player's install: its units drive, are made and fight on its maps.
name = "zero-hour-game"
version = "0.1.0"
api = "1.0"
language = "en"
kind = "mode"
capabilities = ["stats", "orders", "navigation", "abilities", "production", "combat", "projectiles"]
tick_hz = {{ min = 30, max = 30, default = 30 }}
teams = [{teams}]
max_move_speed = "{max_move_speed}"
script_limits = {{ per_call = 1000, player = 1000, think = 1000, mode = 1000 }}

[dependencies]
zero-hour = {{ path = "../zero-hour" }}
"#,
        teams = teams.join(", "),
    )
}

/// What the mode's data is made of: the skirmish's start, the trains and builds, the weapons and
/// armors, and the starting money.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModeImport<'a> {
    pub(crate) start: &'a SkirmishStart,
    pub(crate) production: &'a ProductionImport,
    pub(crate) combat: &'a CombatImport,
    pub(crate) start_money: &'a MoneyChoices,
}

/// The imported package's mode data: what the install gives, over the rules package's fixed
/// data (03-game-scripting.md, Overrides by load order): its name and its teams' names, as its
/// manifest names the teams, the rules package's mode script, its params of what each starts
/// with, `start` (09-zero-hour.md, A skirmish starts with the faction's start objects), and the
/// option of the money it starts with, of `start_money`'s values (09-zero-hour.md, The import
/// writes Zero Hour's money as an option); the armors and weapons of `combat` (09-zero-hour.md)
/// and the trains and builds of `production` (04-capabilities/production.md).
pub(crate) fn mode_data(mode: ModeImport<'_>) -> String {
    let ModeImport {
        start,
        production,
        combat,
        start_money,
    } = mode;
    debug_assert!(
        combat
            .actions
            .keys()
            .all(|name| !production.actions.contains_key(name)),
        "a weapon and a train or build share no name, as one table holds both"
    );
    let teams = (1..=SLOTS)
        .map(|player| (format!("player_{player}"), format!("team-player-{player}")))
        .chain([(String::from("neutral"), String::from("team-neutral"))])
        .map(|(team, name)| (team, TeamToml { name }))
        .collect();
    Toml::write(&ModeToml {
        name: "mode-name",
        scripts: [ScriptToml {
            package: "zero-hour",
            path: "scripts/mode.rhai",
        }],
        teams,
        params: ParamsToml {
            start_building: &start.building,
            start_units: &start.units,
            start_offset: start.offset,
        },
        options: OptionsToml {
            start_money: MoneyToml {
                values: &start_money.values,
                default: start_money.default,
            },
        },
        combat: (!combat.armors.is_empty()).then_some(CombatToml {
            armors: &combat.armors,
        }),
        actions: (!combat.actions.is_empty() || !production.actions.is_empty()).then_some(
            ActionsToml {
                weapons: &combat.actions,
                trains: &production.actions,
            },
        ),
    })
}

/// The imported package's mode data, `data/mode.toml`.
#[derive(Debug, Serialize)]
struct ModeToml<'a> {
    name: &'static str,
    scripts: [ScriptToml; 1],
    teams: BTreeMap<String, TeamToml>,
    params: ParamsToml<'a>,
    options: OptionsToml<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    combat: Option<CombatToml<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actions: Option<ActionsToml<'a>>,
}

/// A script of the mode, by its package and its path.
#[derive(Debug, Serialize)]
struct ScriptToml {
    package: &'static str,
    path: &'static str,
}

/// A team, by the message of its name.
#[derive(Debug, Serialize)]
struct TeamToml {
    name: String,
}

/// The mode script's params of what each player starts with.
#[derive(Debug, Serialize)]
struct ParamsToml<'a> {
    start_building: &'a DeclaredName,
    start_units: &'a [DeclaredName],
    start_offset: Num,
}

/// The mode's options: the money each player starts with.
#[derive(Debug, Serialize)]
struct OptionsToml<'a> {
    start_money: MoneyToml<'a>,
}

/// An option of the money a player starts with: its values, and the one the host gets first.
#[derive(Debug, Serialize)]
struct MoneyToml<'a> {
    values: &'a [u32],
    default: u32,
}

/// The armors of `[combat]`, each a share by damage kind.
#[derive(Debug, Serialize)]
struct CombatToml<'a> {
    armors: &'a BTreeMap<DeclaredName, BTreeMap<DeclaredName, Num>>,
}

/// The mode data's `[actions]`: the weapons, and the trains and builds, in one table.
#[derive(Debug, Serialize)]
struct ActionsToml<'a> {
    #[serde(flatten)]
    weapons: &'a BTreeMap<DeclaredName, ImportedWeapon>,
    #[serde(flatten)]
    trains: &'a BTreeMap<DeclaredName, ImportedAction>,
}

/// The imported package's text in its own language: the names its mode data gives, and each of
/// `texts`, an action's name by its message.
pub(crate) fn mode_text(texts: &BTreeMap<String, String>) -> String {
    let mut text = String::from("mode-name = Zero Hour\n");
    for player in 1..=SLOTS {
        writeln!(text, "team-player-{player} = Player {player}")
            .expect("text writes into a string");
    }
    text.push_str("team-neutral = Neutral\n");
    for (id, value) in texts {
        writeln!(text, "{id} = {}", fluent_value(value)).expect("text writes into a string");
    }
    text
}

/// `text` as a Fluent message's value that gives it back: each line past the first indented, and
/// as a string literal each of what Fluent reads as syntax or drops: a brace, a later line's
/// leading `[`, `*` or `.`, the spaces that start or end a line, and an empty line.
fn fluent_value(text: &str) -> String {
    let mut value = String::with_capacity(text.len());
    for (at, line) in text.split('\n').enumerate() {
        if at > 0 {
            value.push_str("\n    ");
        }
        let core = line.trim_matches(' ');
        if core.is_empty() {
            push_literal(&mut value, line);
            continue;
        }
        let lead = &line[..line.len() - line.trim_start_matches(' ').len()];
        let trail = &line[line.trim_end_matches(' ').len()..];
        if !lead.is_empty() {
            push_literal(&mut value, lead);
        }
        for (place, char) in core.char_indices() {
            match char {
                '{' | '}' => push_literal(&mut value, &core[place..=place]),
                '[' | '*' | '.' if place == 0 && at > 0 && lead.is_empty() => {
                    push_literal(&mut value, &core[place..=place]);
                }
                _ => value.push(char),
            }
        }
        if !trail.is_empty() {
            push_literal(&mut value, trail);
        }
    }
    value
}

/// Pushes `text`, of no quote or backslash, as a Fluent string literal.
fn push_literal(value: &mut String, text: &str) {
    write!(value, r#"{{"{text}"}}"#).expect("text writes into a string");
}

#[cfg(test)]
mod tests {
    use fluent_syntax::ast::{Entry, Expression, InlineExpression, PatternElement};
    use fluent_syntax::parser;
    use serde::de::IgnoredAny;

    use super::*;
    use crate::zero_hour::format::ini::damage_type::DamageType;
    use crate::zero_hour::imported_type::{INFANTRY, LIFE, MONEY, STRUCTURE, VICTORY, WALL};
    use crate::zero_hour::map_import::START;

    #[test]
    fn a_mode_of_no_weapon_armor_or_action_writes_none_of_their_tables() {
        let name = |name| DeclaredName::new(name).unwrap();
        let start = SkirmishStart {
            building: name("center"),
            units: vec![name("dozer")],
            offset: Num::int(2),
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
        assert!(Toml::parse::<IgnoredAny>(&data).is_ok(), "{data}");
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
        #[derive(Debug, serde::Deserialize)]
        #[expect(
            clippy::zero_sized_map_values,
            reason = "the test reads the keys of the rules' tables alone"
        )]
        struct Rules {
            resources: Vec<String>,
            stats: BTreeMap<String, IgnoredAny>,
            pools: BTreeMap<String, IgnoredAny>,
            combat: RulesCombat,
            tags: BTreeMap<String, IgnoredAny>,
            navigation: RulesNavigation,
            slots: Vec<RulesSlot>,
            defaults: RulesDefaults,
        }
        #[derive(Debug, serde::Deserialize)]
        struct RulesCombat {
            damage_kinds: Vec<String>,
            life: String,
        }
        #[derive(Debug, serde::Deserialize)]
        struct RulesNavigation {
            spawn_tags: Vec<String>,
        }
        #[derive(Debug, serde::Deserialize)]
        struct RulesSlot {
            name: String,
        }
        #[derive(Debug, serde::Deserialize)]
        struct RulesDefaults {
            turret: RulesTurret,
        }
        #[derive(Debug, serde::Deserialize)]
        struct RulesTurret {
            slot: String,
        }
        let (_, text) = RULES_FILES
            .into_iter()
            .find(|(path, _)| *path == "data/rules.toml")
            .unwrap();
        let rules: Rules = Toml::parse(std::str::from_utf8(text).unwrap()).unwrap();
        let kinds: Vec<String> = DamageType::ALL.map(|kind| kind.kind().to_string()).into();
        assert_eq!(rules.combat.damage_kinds, kinds);
        assert_eq!(
            (rules.combat.life.as_str(), rules.resources),
            (LIFE, vec![MONEY.to_owned()])
        );
        assert!(rules.stats.contains_key(LIFE) && rules.pools.contains_key(LIFE));
        for tag in [VICTORY, STRUCTURE, WALL, INFANTRY] {
            assert!(rules.tags.contains_key(tag), "{tag}");
        }
        assert_eq!(rules.navigation.spawn_tags, [START]);
        let slots: Vec<&str> = rules.slots.iter().map(|slot| slot.name.as_str()).collect();
        assert_eq!(slots, ["weapon", "command"]);
        assert_eq!(rules.defaults.turret.slot, "weapon");
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
                "mode-name = Zero Hour\n{teams}team-neutral = Neutral\n\
                 action-train-a = A & {{\"{{\"}}B{{\"}}\"}}\n"
            )
        );
    }
}
