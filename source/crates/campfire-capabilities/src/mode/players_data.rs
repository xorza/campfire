use serde::Deserialize;

/// The mode's `[players]`: who may take a slot once the match started, beside the players it
/// started with. A player who left keeps the right to take their slot back while it is reserved,
/// played by a bot or still open; the server's door and the log's records follow these rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PlayersData {
    /// A new player may take an open slot.
    pub late_join: bool,
    /// A new player may take a slot a bot plays; only with `late_join`, which the load checks.
    pub bot_takeover: bool,
    /// What a slot becomes when its player leaves.
    pub leaver: Leaver,
}

/// What the slot of a player who left becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Leaver {
    /// It stays the leaver's alone, and idle.
    #[default]
    Reserve,
    /// A bot plays it.
    Bot,
    /// It is open, for a new player when `late_join` is on.
    Open,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_read_with_their_defaults_and_refuse_what_they_do_not_know() {
        let read = |text: &str| toml::from_str::<PlayersData>(text);
        assert_eq!(read("").unwrap(), PlayersData::default());
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
}
