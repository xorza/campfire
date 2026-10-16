use bevy_ecs::resource::Resource;
use campfire_common::NamedEnum;

campfire_common::enum_all! {
    /// What a player out of play sees of the other teams (design 14, D16). A match holds its
    /// session's as a resource: a player that sees what a spectator sees gives no order.
    #[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum OutView {
        /// What its team sees.
        Team,
        /// What a spectator sees.
        Spectators,
    }
    const ALL;
}

/// As a host's setting names each, and its code its place.
impl NamedEnum for OutView {
    const ALL: &'static [Self] = &Self::ALL;
    const NAMES: &'static [&'static str] = &["team", "spectators"];
}
