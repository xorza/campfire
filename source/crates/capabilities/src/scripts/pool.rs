use campfire_common::PlayerSlot;

/// The pool a script call draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pool {
    /// The calls a player causes: their mode inputs, and the casts of the units they control.
    Player(PlayerSlot),
    /// AI `on_think` calls, and the casts of units no player controls.
    Think,
    /// The mode's own calls: the match start and timers.
    Mode,
}

impl Pool {
    /// The pool of a unit's calls: its player's, when `owner` controls it, or the think pool.
    pub(crate) const fn of(owner: Option<PlayerSlot>) -> Pool {
        match owner {
            Some(slot) => Pool::Player(slot),
            None => Pool::Think,
        }
    }
}
