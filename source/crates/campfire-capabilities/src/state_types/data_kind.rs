/// A group of a match's state that the rules send to clients as one: each kind has its own
/// audience, which design 14 sets, so a state type of one kind goes to one set of clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataKind {
    /// A unit's public state: its type, team, owner, place, body, level, death, destination and
    /// the action under way; every client that sees the unit.
    Unit,
    /// A unit's life pool; every client that sees the unit.
    Life,
    /// A unit's other pools; its owner.
    Pools,
    /// A unit's modifiers, which an aura's carrier shows to the units it reaches; every client
    /// that sees the unit.
    Modifiers,
    /// Each of a unit's action slots' ready tick, charges and toggle; its owner.
    Cooldowns,
    /// A unit's experience and points; its owner.
    Progression,
    /// A unit's inventory; its owner.
    Inventory,
    /// What only the owner's prediction reads: a unit's spawn point, route, progress, modifiers'
    /// clocks and respawn; its owner.
    Prediction,
    /// A unit's script state fields of `sync = "owner"`; its owner.
    UnitStateOwner,
    /// A unit's script state fields of `sync = "team"`; its owner and its team's other players.
    UnitStateTeam,
    /// A unit's script state fields of `sync = "all"`; every client that sees the unit.
    UnitStateAll,
    /// State no client receives.
    Server,
}

impl DataKind {
    /// Whether a client receives state of this kind.
    pub const fn is_sent(self) -> bool {
        !matches!(self, DataKind::Server)
    }
}
