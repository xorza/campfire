use campfire_math::PlayerSlot;

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
