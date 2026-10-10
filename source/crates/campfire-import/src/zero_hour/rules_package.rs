/// Zero Hour's rules package as the importer ships it, `source/packages/zero-hour/`: each file but
/// its index, by its path, which the import writes beside the imported package.
pub(crate) const RULES_FILES: [(&str, &[u8]); 2] = [
    (
        "manifest.toml",
        include_bytes!("../../../../packages/zero-hour/manifest.toml"),
    ),
    (
        "scripts/mode.rhai",
        include_bytes!("../../../../packages/zero-hour/scripts/mode.rhai"),
    ),
];

/// The imported package's manifest: a mode of one player, on the team `player`, beside the
/// `neutral` team every placed object stands on, with no capability, as a map that only stands
/// needs, at the game's 30 logic frames a second, depending on the rules package beside it.
pub(crate) const MODE_MANIFEST: &str = r#"# Zero Hour, imported from the player's install: its maps stand, and no rule runs yet.
name = "zero-hour-game"
version = "0.1.0"
api = "1.0"
language = "en"
kind = "mode"
capabilities = []
tick_hz = { min = 30, max = 30, default = 30 }
teams = [{ name = "player", slots = 1 }, { name = "neutral", slots = 0 }]
backends = { collision = "circles", pathfinding = "grid", visibility = "grid_fog" }
max_move_speed = "1.0"
script_limits = { per_call = 1000, player = 1000, think = 1000, mode = 1000 }

[dependencies]
zero-hour = { path = "../zero-hour" }
"#;

/// The imported package's mode data: the rules package's mode script.
pub(crate) const MODE_DATA: &str = r#"script = { package = "zero-hour", path = "scripts/mode.rhai" }
"#;
