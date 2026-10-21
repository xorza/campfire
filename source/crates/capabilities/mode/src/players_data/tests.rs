use campfire_codec::Toml;

use super::*;

#[test]
fn players_read_with_their_defaults_and_refuse_what_they_do_not_know() {
    let read = |text: &str| Toml::parse::<PlayersData>(text);
    assert_eq!(read("").unwrap(), PlayersData::default());
    assert_eq!(
        PlayersData::default(),
        PlayersData {
            late_join: false,
            bot_takeover: false,
            leaver: AfterLeave::Reserve,
            control: PlayerControl::Units,
        }
    );
    let open = read("late_join = true\nbot_takeover = true\nleaver = \"open\"").unwrap();
    assert_eq!(
        open,
        PlayersData {
            late_join: true,
            bot_takeover: true,
            leaver: AfterLeave::Open,
            control: PlayerControl::Units,
        }
    );
    assert_eq!(
        read("control = \"avatar\"").unwrap().control,
        PlayerControl::Avatar
    );
    let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
    assert_eq!(
        refusal("control = \"hero\""),
        "unknown variant `hero`, expected `units` or `avatar`"
    );
    assert_eq!(read("leaver = \"bot\"").unwrap().leaver, AfterLeave::Bot);
    assert_eq!(
        refusal("leaver = \"kick\""),
        "unknown variant `kick`, expected one of `reserve`, `bot`, `open`"
    );
    let late = refusal("late_joins = true");
    assert!(late.starts_with("unknown field `late_joins`"), "{late}");
}
