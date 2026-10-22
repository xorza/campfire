use campfire_store::internals::TestDir;

use super::*;

#[test]
fn a_bot_is_read_from_its_seat_and_its_file_or_its_name() {
    // The first `=` splits the text, so a file's name may hold another.
    let orders = SeatBotArg::orders("3=bots/a=b.toml").unwrap();
    let path = PathBuf::from("bots/a=b.toml");
    assert_eq!(
        orders,
        SeatBotArg {
            seat: Seat::new(3),
            bot: BotArg::Orders(path),
        }
    );
    let builder = DeclaredName::new("builder").unwrap();
    assert_eq!(
        SeatBotArg::mode("1=builder"),
        Ok(SeatBotArg {
            seat: Seat::new(1),
            bot: BotArg::Mode(builder.clone()),
        })
    );
    assert_eq!(
        SeatBotArg::orders("bot.toml"),
        Err(SeatBotArgError::NotPair("bot.toml".to_owned()))
    );
    assert!(matches!(
        SeatBotArg::mode("x=builder"),
        Err(SeatBotArgError::Seat { text, .. }) if text == "x"
    ));
    assert_eq!(
        SeatBotArg::mode("1=Builder"),
        Err(SeatBotArgError::Name("Builder".to_owned()))
    );

    // A file that is there gives the bot; one that is not, or holds no script, gives its step. A
    // bot of the mode reads no file.
    let test_dir = TestDir::new();
    test_dir.write("bot.toml", "end = 1");
    let file = SeatBotArg {
        seat: Seat::new(2),
        bot: BotArg::Orders(test_dir.path("bot.toml")),
    };
    let read = file.read().unwrap();
    assert_eq!(read.seat, Seat::new(2));
    assert!(matches!(read.plan, BotPlan::Orders(_)));
    let missing = SeatBotArg {
        seat: Seat::new(2),
        bot: BotArg::Orders(test_dir.path("none.toml")),
    };
    assert!(matches!(missing.read(), Err(OrderScriptReadError::Read(_))));
    test_dir.write("bot.toml", "end = \"soon\"");
    assert!(matches!(
        file.read(),
        Err(OrderScriptReadError::Script { .. })
    ));
    let mode = SeatBotArg::mode("0=builder").unwrap().read().unwrap();
    assert!(matches!(mode.plan, BotPlan::Mode(name) if name == builder));
}
