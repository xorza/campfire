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
    assert!(read("control = \"hero\"").is_err());
    assert_eq!(read("leaver = \"bot\"").unwrap().leaver, AfterLeave::Bot);
    assert!(read("leaver = \"kick\"").is_err());
    assert!(read("late_joins = true").is_err());
}
