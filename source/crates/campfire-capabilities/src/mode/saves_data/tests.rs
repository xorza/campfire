use super::*;

#[test]
fn saves_read_with_their_defaults_and_refuse_what_they_do_not_know() {
    let read = |text: &str| toml::from_str::<SavesData>(text);
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
    assert!(read("by = \"anyone\"").is_err());
    assert!(read("autosave_ms = 0").is_err());
    assert!(read("autosave = 1").is_err());
}
