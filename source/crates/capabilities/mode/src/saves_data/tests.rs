use campfire_codec::Toml;

use super::*;

#[test]
fn saves_read_with_their_defaults_and_refuse_what_they_do_not_know() {
    let read = |text: &str| Toml::parse::<SavesData>(text);
    assert_eq!(
        read("").unwrap(),
        SavesData {
            by: SaveBy::Player,
            autosave_ms: None,
        }
    );
    assert_eq!(
        read("by = \"mode\"\nautosave_ms = 12000").unwrap(),
        SavesData {
            by: SaveBy::Mode,
            autosave_ms: NonZeroU32::new(12_000),
        }
    );
    let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
    assert_eq!(
        refusal("by = \"anyone\""),
        "unknown variant `anyone`, expected `player` or `mode`"
    );
    assert_eq!(
        refusal("autosave_ms = 0"),
        "invalid value: integer `0`, expected a nonzero u32"
    );
    let autosave = refusal("autosave = 1");
    assert!(
        autosave.starts_with("unknown field `autosave`"),
        "{autosave}"
    );
}
