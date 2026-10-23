use campfire_store::internals::TestDir;

use super::*;

#[test]
fn a_bot_is_read_from_its_player_slot_and_its_file_or_its_name() {
    // The first `=` splits the text, so a file's name may hold another.
    let orders = PlayerSlotBotArg::orders("3=bots/a=b.toml").unwrap();
    let path = PathBuf::from("bots/a=b.toml");
    assert_eq!(
        orders,
        PlayerSlotBotArg {
            player_slot: PlayerSlot::new(3),
            bot: BotArg::Orders(OrderFile::from(path)),
        }
    );
    let builder = DeclaredName::new("builder").unwrap();
    assert_eq!(
        PlayerSlotBotArg::mode("1=builder"),
        Ok(PlayerSlotBotArg {
            player_slot: PlayerSlot::new(1),
            bot: BotArg::Mode(builder.clone()),
        })
    );
    assert_eq!(
        PlayerSlotBotArg::orders("bot.toml"),
        Err(PlayerSlotBotArgError::NotPair("bot.toml".to_owned()))
    );
    assert!(matches!(
        PlayerSlotBotArg::mode("x=builder"),
        Err(PlayerSlotBotArgError::PlayerSlot { text, .. }) if text == "x"
    ));
    assert_eq!(
        PlayerSlotBotArg::mode("1=Builder"),
        Err(PlayerSlotBotArgError::Name("Builder".to_owned()))
    );

    // A file that is there gives the bot; one that is not, or holds no script, gives its step. A
    // bot of the mode reads no file.
    let test_dir = TestDir::new();
    test_dir.write("bot.toml", "end = 1");
    let file = PlayerSlotBotArg {
        player_slot: PlayerSlot::new(2),
        bot: BotArg::Orders(OrderFile::from(test_dir.path("bot.toml"))),
    };
    let read = file.read().unwrap();
    assert_eq!(read.player_slot, PlayerSlot::new(2));
    assert!(matches!(read.plan, BotPlan::Orders(_)));
    let missing = PlayerSlotBotArg {
        player_slot: PlayerSlot::new(2),
        bot: BotArg::Orders(OrderFile::from(test_dir.path("none.toml"))),
    };
    assert!(matches!(missing.read(), Err(OrderFileError::Read(_))));
    test_dir.write("bot.toml", "end = \"soon\"");
    assert!(matches!(file.read(), Err(OrderFileError::Script { .. })));
    let mode = PlayerSlotBotArg::mode("0=builder").unwrap().read().unwrap();
    assert!(matches!(mode.plan, BotPlan::Mode(name) if name == builder));
}
