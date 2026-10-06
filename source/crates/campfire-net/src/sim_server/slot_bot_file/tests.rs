use std::fs;

use tempfile::TempDir;

use super::*;

#[test]
fn a_bot_is_read_from_its_slot_and_its_file() {
    // The first `=` splits the text, so a file's name may hold another.
    let bot: SlotBotFile = "3=bots/a=b.toml".parse().unwrap();
    assert_eq!(
        bot,
        SlotBotFile {
            slot: PlayerSlot::new(3),
            path: PathBuf::from("bots/a=b.toml"),
        }
    );
    assert_eq!(
        "bot.toml".parse::<SlotBotFile>(),
        Err(SlotBotFileError::NotPair("bot.toml".to_owned()))
    );
    assert!(matches!(
        "x=bot.toml".parse::<SlotBotFile>(),
        Err(SlotBotFileError::Slot { text, .. }) if text == "x"
    ));

    // A file that is there gives the bot; one that is not, or holds no script, gives its step.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bot.toml");
    fs::write(&path, "end = 1").unwrap();
    let file = SlotBotFile {
        slot: PlayerSlot::new(2),
        path,
    };
    let read = file.read().unwrap();
    assert_eq!(read.slot, PlayerSlot::new(2));
    let missing = SlotBotFile {
        slot: PlayerSlot::new(2),
        path: dir.path().join("none.toml"),
    };
    assert!(matches!(
        missing.read(),
        Err(OrderScriptReadError::Read { .. })
    ));
    fs::write(&file.path, "end = \"soon\"").unwrap();
    assert!(matches!(
        file.read(),
        Err(OrderScriptReadError::Script { .. })
    ));
}
