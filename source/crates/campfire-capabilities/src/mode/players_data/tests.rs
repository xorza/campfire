use super::*;

#[test]
fn players_read_with_their_defaults_and_refuse_what_they_do_not_know() {
    let read = |text: &str| toml::from_str::<PlayersData>(text);
    assert_eq!(read("").unwrap(), PlayersData::default());
    assert_eq!(PlayersData::DEFAULT, PlayersData::default());
    assert_eq!(
        PlayersData::default(),
        PlayersData {
            late_join: false,
            bot_takeover: false,
            leaver: Leaver::Reserve,
        }
    );
    let open = read("late_join = true\nbot_takeover = true\nleaver = \"open\"").unwrap();
    assert_eq!(
        open,
        PlayersData {
            late_join: true,
            bot_takeover: true,
            leaver: Leaver::Open,
        }
    );
    assert_eq!(read("leaver = \"bot\"").unwrap().leaver, Leaver::Bot);
    assert!(read("leaver = \"kick\"").is_err());
    assert!(read("late_joins = true").is_err());
}
