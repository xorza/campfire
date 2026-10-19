use campfire_store::internals::TestDir;

use super::*;

#[test]
fn a_bot_is_read_from_its_seat_and_its_file() {
    // The first `=` splits the text, so a file's name may hold another.
    let bot: SeatBotFile = "3=bots/a=b.toml".parse().unwrap();
    assert_eq!(
        bot,
        SeatBotFile {
            seat: Seat::new(3),
            path: PathBuf::from("bots/a=b.toml"),
        }
    );
    assert_eq!(
        "bot.toml".parse::<SeatBotFile>(),
        Err(SeatBotFileError::NotPair("bot.toml".to_owned()))
    );
    assert!(matches!(
        "x=bot.toml".parse::<SeatBotFile>(),
        Err(SeatBotFileError::Seat { text, .. }) if text == "x"
    ));

    // A file that is there gives the bot; one that is not, or holds no script, gives its step.
    let test_dir = TestDir::new();
    test_dir.write("bot.toml", "end = 1");
    let file = SeatBotFile {
        seat: Seat::new(2),
        path: test_dir.path("bot.toml"),
    };
    let read = file.read().unwrap();
    assert_eq!(read.seat, Seat::new(2));
    let missing = SeatBotFile {
        seat: Seat::new(2),
        path: test_dir.path("none.toml"),
    };
    assert!(matches!(missing.read(), Err(OrderScriptReadError::Read(_))));
    test_dir.write("bot.toml", "end = \"soon\"");
    assert!(matches!(
        file.read(),
        Err(OrderScriptReadError::Script { .. })
    ));
}
