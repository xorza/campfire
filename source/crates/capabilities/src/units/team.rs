use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit's team: its index in the mode's list of teams. Units of different teams are enemies, so
/// a neutral team for camps is one more index, an enemy of every other.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Team(u8);

impl Team {
    /// The most teams a match holds, the neutral one included: one bit each in a `TeamSet`.
    pub(crate) const LIMIT: usize = 64;

    /// The team at `index`, which is below `LIMIT`.
    pub const fn new(index: u8) -> Team {
        assert!(
            (index as usize) < Team::LIMIT,
            "a team index is below the limit"
        );
        Team(index)
    }

    pub const fn index(self) -> u8 {
        self.0
    }

    pub const fn is_enemy_of(self, other: Team) -> bool {
        self.0 != other.0
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "units.team";
}

/// A snapshot is untrusted, so a team past the limit fails to decode.
impl<'de> Deserialize<'de> for Team {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Team, D::Error> {
        let index = u8::deserialize(deserializer)?;
        if usize::from(index) >= Team::LIMIT {
            return Err(D::Error::custom("a team past the limit"));
        }
        Ok(Team(index))
    }
}
