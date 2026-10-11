/// A group of a match's state that the rules send to clients as one: each kind has its own
/// audience, which design 14 sets, so a state type of one kind goes to one set of clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataKind {
    /// A unit's public state: its type, team, owner, place, body, level, death, destination and
    /// the action under way; every client that sees the unit.
    Unit,
    /// A unit's pools, the life pool among them; every client that sees the unit.
    Life,
    /// A unit's modifiers, which an aura's carrier shows to the units it reaches; every client
    /// that sees the unit.
    Modifiers,
    /// A unit's experience and points; its owner.
    Progression,
    /// A unit's inventory; its owner.
    Inventory,
    /// What only the owner's prediction reads: a unit's spawn point, route, progress, modifiers'
    /// clocks and respawn; its owner.
    Prediction,
    /// State no client receives.
    Server,
}
