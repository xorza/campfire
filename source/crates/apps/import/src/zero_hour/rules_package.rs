use std::collections::BTreeMap;
use std::fmt::Write as _;

use campfire_codec::Toml;
use campfire_math::Num;
use campfire_values::DeclaredName;
use serde::Serialize;

use crate::zero_hour::armor_ini::DAMAGE_TYPES;
use crate::zero_hour::combat_import::{CombatImport, WEAPON};
use crate::zero_hour::imported_type::{LIFE, VICTORY};
use crate::zero_hour::map_import::START;
use crate::zero_hour::production_import::{COMMAND, MONEY, ProductionImport};
use crate::zero_hour::skirmish_start::SkirmishStart;

/// Zero Hour's rules package as the importer ships it, `source/packages/zero-hour/`: each file but
/// its index, by its path, which the import writes beside the imported package.
pub(crate) const RULES_FILES: [(&str, &[u8]); 2] = [
    (
        "manifest.toml",
        include_bytes!("../../../../../packages/zero-hour/manifest.toml"),
    ),
    (
        "scripts/mode.rhai",
        include_bytes!("../../../../../packages/zero-hour/scripts/mode.rhai"),
    ),
];

/// The players of a skirmish, each its own team, as the original's default skirmish holds them
/// (design 19, D1).
pub(crate) const PLAYERS: u8 = 8;

/// The imported package's manifest: a mode of eight players, each on a team of its own, beside
/// the `neutral` team every placed object stands on, whose units drive, are made and fight
/// (designs 17, 18 and 19): of `stats`, `orders`, `navigation`, `abilities`, `production` and
/// `combat`, no unit faster than `max_move_speed` meters a second, at the game's 30 logic frames a
/// second, depending on the rules package beside it.
pub(crate) fn mode_manifest(max_move_speed: Num) -> String {
    let teams: Vec<String> = (1..=PLAYERS)
        .map(|player| format!("{{ name = \"player_{player}\", slots = 1 }}"))
        .chain(["{ name = \"neutral\", slots = 0 }".to_owned()])
        .collect();
    format!(
        r#"# Zero Hour, imported from the player's install: its units drive, are made and fight on its maps.
name = "zero-hour-game"
version = "0.1.0"
api = "1.0"
language = "en"
kind = "mode"
capabilities = ["stats", "orders", "navigation", "abilities", "production", "combat"]
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
    pub(crate) start_money: u32,
}

/// The imported package's mode data: its name and its teams' names, the rules package's mode
/// script, the player resource `money`, the stats its unit types drive by and the life they live
/// by, the pool of that life, its combat of the game's damage types and armors, the tag of
/// victory, the markers its players start at, the state `ended` by which its script ends the
/// match once, its params of what each starts with, `start` (design 17, D6), and the money
/// `start_money`; the slot kinds `weapon` and `command`, the weapons of `combat` (design 19) and
/// the trains and builds of `production` (design 18).
pub(crate) fn mode_data(mode: ModeImport<'_>) -> String {
    let ModeImport {
        start,
        production,
        combat,
        start_money,
    } = mode;
    let units: Vec<String> = start
        .units
        .iter()
        .map(|unit| format!("\"{unit}\""))
        .collect();
    let kinds: Vec<String> = DAMAGE_TYPES
        .iter()
        .map(|kind| format!("\"{}\"", CombatImport::kind(kind)))
        .collect();
    let mut teams = String::new();
    for player in 1..=PLAYERS {
        write!(
            teams,
            "[teams.player_{player}]\nname = \"team-player-{player}\"\n\n"
        )
        .expect("text writes into a string");
    }
    let armors = ArmorsToml {
        combat: CombatToml {
            armors: &combat.armors,
        },
    }
    .write();
    let weapons = ActionsToml {
        actions: &combat.actions,
    }
    .write();
    let trains = ActionsToml {
        actions: &production.actions,
    }
    .write();
    format!(
        r#"name = "mode-name"
script = {{ package = "zero-hour", path = "scripts/mode.rhai" }}
resources = ["{MONEY}"]

[stats]
move_speed = {{}}
turn_rate = {{}}
{LIFE} = {{}}

[pools.{LIFE}]
max = "{LIFE}"

[combat]
damage_kinds = [{kinds}]
life = "{LIFE}"

[tags.{VICTORY}]

{teams}[teams.neutral]
name = "team-neutral"

[navigation]
spawn_tags = ["{START}"]

[[slots]]
name = "{WEAPON}"

[[slots]]
name = "{COMMAND}"

[state]
ended = {{ type = "bool", default = false }}

[params]
start_building = "{building}"
start_units = [{units}]
start_offset = "{offset}"
start_money = {start_money}

{armors}
{weapons}
{trains}"#,
        kinds = kinds.join(", "),
        building = start.building,
        units = units.join(", "),
        offset = start.offset,
    )
}

/// The mode data's armors, `[combat.armors.<name>]`.
#[derive(Debug, Serialize)]
struct ArmorsToml<'a> {
    combat: CombatToml<'a>,
}

/// The armors of `[combat]`, each a share by damage kind.
#[derive(Debug, Serialize)]
struct CombatToml<'a> {
    armors: &'a BTreeMap<DeclaredName, BTreeMap<DeclaredName, Num>>,
}

/// The mode data's `[actions]`: weapons, or trains and builds.
#[derive(Debug, Serialize)]
struct ActionsToml<'a, A> {
    actions: &'a BTreeMap<DeclaredName, A>,
}

impl ArmorsToml<'_> {
    /// Its text; none for no armor, as an empty table writes `armors = {}` under a second
    /// `[combat]`.
    fn write(&self) -> String {
        if self.combat.armors.is_empty() {
            String::new()
        } else {
            Toml::write(self)
        }
    }
}

impl<A: Serialize> ActionsToml<'_, A> {
    /// Its text; none for no action, as an empty table writes `actions = {}`, a key of the table
    /// above it.
    fn write(&self) -> String {
        if self.actions.is_empty() {
            String::new()
        } else {
            Toml::write(self)
        }
    }
}

/// The imported package's text in its own language: the names its mode data gives, and each of
/// `texts`, an action's name by its message.
pub(crate) fn mode_text(texts: &BTreeMap<String, String>) -> String {
    let mut text = String::from("mode-name = Zero Hour\n");
    for player in 1..=PLAYERS {
        writeln!(text, "team-player-{player} = Player {player}")
            .expect("text writes into a string");
    }
    text.push_str("team-neutral = Neutral\n");
    for (id, value) in texts {
        writeln!(text, "{id} = {}", fluent_value(value)).expect("text writes into a string");
    }
    text
}

/// `text` as a Fluent message's value that gives it back: each brace a string literal, each line
/// past the first indented, a line's leading `[`, `*` or `.`, which Fluent reads as syntax there,
/// a string literal too, and an empty text an empty literal.
fn fluent_value(text: &str) -> String {
    if text.is_empty() {
        return r#"{""}"#.to_owned();
    }
    let mut value = String::with_capacity(text.len());
    for (at, line) in text.split('\n').enumerate() {
        if at > 0 {
            value.push_str("\n    ");
        }
        for (place, char) in line.chars().enumerate() {
            match char {
                '{' | '}' => write!(value, r#"{{"{char}"}}"#).expect("text writes into a string"),
                '[' | '*' | '.' if place == 0 && at > 0 => {
                    write!(value, r#"{{"{char}"}}"#).expect("text writes into a string");
                }
                _ => value.push(char),
            }
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use serde::de::IgnoredAny;

    use super::*;

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
            start_money: 100,
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
