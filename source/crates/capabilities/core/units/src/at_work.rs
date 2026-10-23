use bevy_ecs::component::Component;

/// Whether a unit works in a loop that holds it where it stands, as a builder with a build: the
/// capability that runs the loop writes it, so what moves an idle unit out of a walker's way reads
/// this, not that capability's state, and leaves a busy one, as Zero Hour tells no busy unit to
/// move out of the way (04-capabilities/navigation.md, An idle friend steps aside). Derived,
/// never state, and a server's alone, as only a server moves a unit aside.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AtWork(bool);

impl AtWork {
    pub const fn new(at_work: bool) -> Self {
        Self(at_work)
    }

    pub const fn get(self) -> bool {
        self.0
    }
}
