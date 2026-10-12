use crate::state_types::recipients::Recipients;

/// A group of a match's state that the rules send to clients as one: each kind has its own
/// audience, `audience`, so a state type of one kind goes to one set of clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// The mode's script state fields of `sync = "all"`, on the match's entity.
    ModeState,
    /// A player's resources, on its entity.
    Resources,
    /// A player's supply: what its units and queued trains use, and its cap.
    Supply,
    /// A producer's train queue.
    Queues,
    /// The modifiers a player holds for its units, on its entity.
    PlayerModifiers,
    /// What only the owner's prediction reads: a unit's spawn point, route, progress, modifiers'
    /// clocks and respawn.
    Prediction,
    /// A unit's script state fields of `sync = "owner"`.
    UnitStateOwner,
    /// A unit's script state fields of `sync = "team"`.
    UnitStateTeam,
    /// A unit's script state fields of `sync = "all"`.
    UnitStateAll,
    /// State no client receives.
    Server,
}

impl DataKind {
    /// Every kind, in the order the enum declares them, which the session's terms keep.
    pub const ALL: [DataKind; 18] = [
        DataKind::Unit,
        DataKind::Life,
        DataKind::Pools,
        DataKind::Modifiers,
        DataKind::Cooldowns,
        DataKind::Progression,
        DataKind::Inventory,
        DataKind::Match,
        DataKind::ModeState,
        DataKind::Resources,
        DataKind::Supply,
        DataKind::Queues,
        DataKind::PlayerModifiers,
        DataKind::Prediction,
        DataKind::UnitStateOwner,
        DataKind::UnitStateTeam,
        DataKind::UnitStateAll,
        DataKind::Server,
    ];

    /// Its name in a mode's `[visibility]` and a host's settings.
    pub const fn name(self) -> &'static str {
        match self {
            DataKind::Unit => "unit",
            DataKind::Life => "life",
            DataKind::Pools => "pools",
            DataKind::Modifiers => "modifiers",
            DataKind::Cooldowns => "cooldowns",
            DataKind::Progression => "progression",
            DataKind::Inventory => "inventory",
            DataKind::Match => "match",
            DataKind::ModeState => "mode_state",
            DataKind::Resources => "resources",
            DataKind::Supply => "supply",
            DataKind::Queues => "queues",
            DataKind::PlayerModifiers => "player_modifiers",
            DataKind::Prediction => "prediction",
            DataKind::UnitStateOwner => "unit_state_owner",
            DataKind::UnitStateTeam => "unit_state_team",
            DataKind::UnitStateAll => "unit_state_all",
            DataKind::Server => "server",
        }
    }

    /// The kind of the name `name`; `None` for no kind's.
    pub fn named(name: &str) -> Option<DataKind> {
        DataKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// Whether a client receives state of this kind.
    pub const fn is_sent(self) -> bool {
        !matches!(self, DataKind::Server)
    }

    /// Who beside the owner receives this kind unless the mode says (design 14, D4): every
    /// client that holds the entity for public state, the owner alone for its own; no one at all
    /// for the server's, which `is_sent` tells. Vision decides who holds a unit.
    pub const fn audience(self) -> Recipients {
        match self {
            DataKind::Unit
            | DataKind::Life
            | DataKind::Modifiers
            | DataKind::Match
            | DataKind::ModeState
            | DataKind::PlayerModifiers
            | DataKind::UnitStateAll => Recipients::ALL,
            DataKind::UnitStateTeam => Recipients::ALLIES,
            DataKind::Pools
            | DataKind::Cooldowns
            | DataKind::Progression
            | DataKind::Inventory
            | DataKind::Resources
            | DataKind::Supply
            | DataKind::Queues
            | DataKind::Prediction
            | DataKind::UnitStateOwner
            | DataKind::Server => Recipients::NONE,
        }
    }

    /// The recipients a mode, or its host, may add to this kind's audience or take from it
    /// (design 14, D4); the others keep their place in `audience`. None for a kind vision alone
    /// decides, the owner's prediction, and the server's.
    pub const fn settable(self) -> Recipients {
        match self {
            DataKind::Life
            | DataKind::Modifiers
            | DataKind::PlayerModifiers
            | DataKind::ModeState => Recipients::ENEMIES.union(Recipients::SPECTATORS),
            DataKind::Pools
            | DataKind::Cooldowns
            | DataKind::Progression
            | DataKind::Inventory
            | DataKind::Resources
            | DataKind::Supply
            | DataKind::Queues => Recipients::ALL,
            DataKind::UnitStateOwner | DataKind::UnitStateTeam | DataKind::UnitStateAll => {
                Recipients::SPECTATORS
            }
            DataKind::Unit | DataKind::Match | DataKind::Prediction | DataKind::Server => {
                Recipients::NONE
            }
        }
    }

    /// Its place in `ALL`.
    pub(crate) const fn index(self) -> usize {
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
