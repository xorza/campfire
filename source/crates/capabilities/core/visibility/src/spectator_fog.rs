campfire_common::enum_all! {
    /// What a session's spectators see (design 14, D15), by the name a host's setting gives it.
    /// A member's place is its byte in a visibility's encoding, so a new member goes last.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum SpectatorFog {
        /// Every unit, through any fog.
        All => "all",
        /// One team's view, or all teams' together, as the spectator chooses.
        Teams => "teams",
    }
    const ALL;
}
