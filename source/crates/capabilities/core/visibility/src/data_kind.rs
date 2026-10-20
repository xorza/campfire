use campfire_common::EnumAll;

use crate::recipients::Recipients;

/// A group of a match's state that the rules send to clients as one: each kind has its own
/// audience, `audience`, so a state type of one kind goes to one set of clients. The order its
/// kinds are declared in is the one the session's terms keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum DataKind {
    /// A unit's public state: its type, team, owner, place, body, level, death, destination and
    /// the action under way.
    Unit,
    /// A unit's life pool.
    Life,
    /// A unit's other pools.
    Pools,
    /// A unit's modifiers, which an aura's carrier shows to the units it reaches.
    Modifiers,
    /// Each of a unit's action slots' ready tick, charges and toggle.
    Cooldowns,
    /// A unit's experience and points.
    Progression,
    /// A unit's inventory.
    Inventory,
    /// The match's public state, on the match's entity and each player's: the teams' relations,
    /// the match's end, the players' slots.
    Match,
    /// The mode's script state fields of `audience = "all"`, on the match's entity.
    ModeState,
    /// A player's resources, on its entity.
    Resources,
    /// A player's supply: what its units and queued trains use, and its cap.
    Supply,
    /// A producer's train queue.
    Queues,
    /// The modifiers a player holds for its units, on its entity.
    PlayerModifiers,
    /// What only the owner's prediction reads: a unit's spawn point, route, progress,
    /// modifiers' clocks and respawn.
    Prediction,
    /// A unit's script state fields of `audience = "owner"`.
    UnitStateOwner,
    /// A unit's script state fields of `audience = "team"`.
    UnitStateTeam,
    /// A unit's script state fields of `audience = "all"`.
    UnitStateAll,
    /// A player's script state fields of `audience = "owner"`, on its entity.
    PlayerStateOwner,
    /// A player's script state fields of `audience = "team"`, on its entity.
    PlayerStateTeam,
    /// A player's script state fields of `audience = "all"`, on its entity.
    PlayerStateAll,
    /// State no client receives.
    Server,
}

impl DataKind {
    /// Its name in a mode's `[visibility]` and a host's settings.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Life => "life",
            Self::Pools => "pools",
            Self::Modifiers => "modifiers",
            Self::Cooldowns => "cooldowns",
            Self::Progression => "progression",
            Self::Inventory => "inventory",
            Self::Match => "match",
            Self::ModeState => "mode_state",
            Self::Resources => "resources",
            Self::Supply => "supply",
            Self::Queues => "queues",
            Self::PlayerModifiers => "player_modifiers",
            Self::Prediction => "prediction",
            Self::UnitStateOwner => "unit_state_owner",
            Self::UnitStateTeam => "unit_state_team",
            Self::UnitStateAll => "unit_state_all",
            Self::PlayerStateOwner => "player_state_owner",
            Self::PlayerStateTeam => "player_state_team",
            Self::PlayerStateAll => "player_state_all",
            Self::Server => "server",
        }
    }

    /// The kind of the name `name`; `None` for no kind's.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// Whether a client receives state of this kind.
    pub const fn is_sent(self) -> bool {
        !matches!(self, Self::Server)
    }

    /// Who beside the owner receives this kind unless the mode says (10-replication-audiences.md,
    /// Each kind has a default audience, and a mode may set it): every client that holds the entity
    /// for public state, the owner alone for its own; no one at all for the server's, which
    /// `is_sent` tells. Vision decides who holds a unit.
    pub const fn audience(self) -> Recipients {
        match self {
            Self::Unit
            | Self::Life
            | Self::Modifiers
            | Self::Match
            | Self::ModeState
            | Self::PlayerModifiers
            | Self::UnitStateAll
            | Self::PlayerStateAll => Recipients::ALL,
            Self::UnitStateTeam | Self::PlayerStateTeam => Recipients::ALLIES,
            Self::Pools
            | Self::Cooldowns
            | Self::Progression
            | Self::Inventory
            | Self::Resources
            | Self::Supply
            | Self::Queues
            | Self::Prediction
            | Self::UnitStateOwner
            | Self::PlayerStateOwner
            | Self::Server => Recipients::NONE,
        }
    }

    /// The recipients a mode, or its host, may add to this kind's audience or take from it
    /// (10-replication-audiences.md, Each kind has a default audience, and a mode may set it); the
    /// others keep their place in `audience`. None for a kind vision alone decides, the owner's
    /// prediction, and the server's.
    pub const fn settable(self) -> Recipients {
        match self {
            Self::Life | Self::Modifiers | Self::PlayerModifiers | Self::ModeState => {
                Recipients::ENEMIES.union(Recipients::SPECTATORS)
            }
            Self::Pools
            | Self::Cooldowns
            | Self::Progression
            | Self::Inventory
            | Self::Resources
            | Self::Supply
            | Self::Queues => Recipients::ALL,
            Self::UnitStateOwner
            | Self::UnitStateTeam
            | Self::UnitStateAll
            | Self::PlayerStateOwner
            | Self::PlayerStateTeam
            | Self::PlayerStateAll => Recipients::SPECTATORS,
            Self::Unit | Self::Match | Self::Prediction | Self::Server => Recipients::NONE,
        }
    }

    /// Its place in `ALL`.
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_has_its_place_and_its_name() {
        for (at, kind) in DataKind::ALL.into_iter().enumerate() {
            assert_eq!(kind.index(), at);
            assert_eq!(DataKind::named(kind.name()), Some(kind));
        }
        assert_eq!(DataKind::named("Pools"), None);
        assert_eq!(DataKind::named("unit_state"), None);
        // A kind the server keeps reaches no one, and no mode may send it.
        assert_eq!(DataKind::Server.audience(), Recipients::NONE);
        assert_eq!(DataKind::Server.settable(), Recipients::NONE);
    }
}
