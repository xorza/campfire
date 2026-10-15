use std::collections::BTreeMap;
use std::fmt::Write as _;

use campfire_codec::Toml;
use campfire_math::Num;
use campfire_values::DeclaredName;
use serde::Serialize;

use crate::zero_hour::map_import::START;
use crate::zero_hour::production_import::{COMMAND, ImportedAction, MONEY, ProductionImport};
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

/// The imported package's manifest: a mode of one player, on the team `player`, beside the
/// `neutral` team every placed object stands on, whose units drive (design 17): of `stats`,
/// `orders` and `navigation`, no unit faster than `max_move_speed` meters a second, at the
/// game's 30 logic frames a second, depending on the rules package beside it.
pub(crate) fn mode_manifest(max_move_speed: Num) -> String {
    format!(
        r#"# Zero Hour, imported from the player's install: its units drive on its maps.
name = "zero-hour-game"
version = "0.1.0"
api = "1.0"
language = "en"
kind = "mode"
capabilities = ["stats", "orders", "navigation", "abilities", "production"]
tick_hz = {{ min = 30, max = 30, default = 30 }}
teams = [{{ name = "player", slots = 1 }}, {{ name = "neutral", slots = 0 }}]
max_move_speed = "{max_move_speed}"
script_limits = {{ per_call = 1000, player = 1000, think = 1000, mode = 1000 }}

[dependencies]
zero-hour = {{ path = "../zero-hour" }}
"#
    )
}

/// The imported package's mode data: its name and its teams' names, the rules package's mode
/// script, the player resource `money`, the stats its unit types drive by, the markers its
/// players start at, its params of what each starts with, `start` (design 17, D6), and the
/// money `start_money`; the slot kind `command` and the trains and builds of `production`
/// (design 18).
pub(crate) fn mode_data(
    start: &SkirmishStart,
    production: &ProductionImport,
    start_money: u32,
) -> String {
    let units: Vec<String> = start
        .units
        .iter()
        .map(|unit| format!("\"{unit}\""))
        .collect();
    // An empty table would write `actions = {}`, a key of `[params]` above it.
    let actions = if production.actions.is_empty() {
        String::new()
    } else {
        Toml::write(&ActionsToml {
            actions: &production.actions,
        })
    };
    format!(
        r#"name = "mode-name"
script = {{ package = "zero-hour", path = "scripts/mode.rhai" }}
resources = ["{MONEY}"]

[stats]
move_speed = {{}}
turn_rate = {{}}

[teams.player]
name = "team-player"

[teams.neutral]
name = "team-neutral"

[navigation]
spawn_tags = ["{START}"]

[[slots]]
name = "{COMMAND}"

[params]
start_building = "{building}"
start_units = [{units}]
start_offset = "{offset}"
start_money = {start_money}

{actions}"#,
        building = start.building,
        units = units.join(", "),
        offset = start.offset,
    )
}

/// The mode data's `[actions]`, each a train or a build.
#[derive(Debug, Serialize)]
struct ActionsToml<'a> {
    actions: &'a BTreeMap<DeclaredName, ImportedAction>,
}

/// The imported package's text in its own language: the names its mode data gives, and each of
/// `texts`, an action's name by its message.
pub(crate) fn mode_text(texts: &BTreeMap<String, String>) -> String {
    let mut text =
        String::from("mode-name = Zero Hour\nteam-player = Player\nteam-neutral = Neutral\n");
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
    use super::*;

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
        // A mode's text holds its names, then each action's.
        let texts = [("action-train-a".to_owned(), "A & {B}".to_owned())].into();
        assert_eq!(
            mode_text(&texts),
            "mode-name = Zero Hour\nteam-player = Player\nteam-neutral = Neutral\n\
             action-train-a = A & {\"{\"}B{\"}\"}\n"
        );
    }
}
