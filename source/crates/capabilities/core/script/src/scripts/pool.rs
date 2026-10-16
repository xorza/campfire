use campfire_common::PlayerSlot;

/// The pool a script call draws from, as design 02's table gives them; its `systems` pool, of
/// `on_system`, is planned, and `calc_damage`, `calc_heal` and `on_generate` draw from none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    /// The calls a player causes: their units' actions, with those actions' deliveries and
    /// modifiers, and their mode inputs.
    Player(PlayerSlot),
    /// AI `on_think` calls, and an action's or a modifier's hooks whose source no player
    /// controls, or is gone.
    Think,
    /// The mode's own hooks: the match start, timers, players' joins and leaves, deaths and
    /// `on_level_up`; and a modifier's hooks when it has no source.
    Mode,
}

impl Pool {
    /// The pool of a unit's calls: its player's, when `owner` controls it, or the think pool.
    pub const fn of(owner: Option<PlayerSlot>) -> Self {
        match owner {
            Some(slot) => Self::Player(slot),
            None => Self::Think,
        }
    }
}
