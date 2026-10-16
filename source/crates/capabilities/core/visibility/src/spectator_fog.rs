use campfire_common::NamedEnum;

campfire_common::enum_all! {
    /// What a session's spectators see (design 14, D15).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum SpectatorFog {
        /// Every unit, through any fog.
        All,
        /// One team's view, or all teams' together, as the spectator chooses.
        Teams,
    }
    const ALL;
}

/// As a host's setting names each, and its code its place.
impl NamedEnum for SpectatorFog {
    const ALL: &'static [Self] = &Self::ALL;
    const NAMES: &'static [&'static str] = &["all", "teams"];
}
