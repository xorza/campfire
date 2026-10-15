use campfire_math::Num;

use crate::zero_hour::map_import::START;
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
capabilities = ["stats", "orders", "navigation"]
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
/// script, the stats its unit types drive by, the markers its players start at, and its params
/// of what each starts with, `start` (design 17, D6).
pub(crate) fn mode_data(start: &SkirmishStart) -> String {
    let units: Vec<String> = start
        .units
        .iter()
        .map(|unit| format!("\"{unit}\""))
        .collect();
    format!(
        r#"name = "mode-name"
script = {{ package = "zero-hour", path = "scripts/mode.rhai" }}

[stats]
move_speed = {{}}
turn_rate = {{}}

[teams.player]
name = "team-player"

[teams.neutral]
name = "team-neutral"

[navigation]
spawn_tags = ["{START}"]

[params]
start_building = "{building}"
start_units = [{units}]
start_offset = "{offset}"
"#,
        building = start.building,
        units = units.join(", "),
        offset = start.offset,
    )
}

/// The imported package's text in its own language, of the names its mode data gives.
pub(crate) const MODE_TEXT: &str =
    "mode-name = Zero Hour\nteam-player = Player\nteam-neutral = Neutral\n";
