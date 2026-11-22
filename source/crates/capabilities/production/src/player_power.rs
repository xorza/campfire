use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// A player's power, on its entity, as its kind's audience sees it: what its living, complete
/// units give and take, each at least 0 (power.md, A unit's power is a stat). The server derives
/// it as each tick ends and sends it on each change; no hash or snapshot holds it, and a client
/// reads its own as the server sent it (power.md, The count runs as each tick ends).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerPower {
    given: Num,
    used: Num,
}

impl PlayerPower {
    /// The power of a player whose units give `given` and take `used`, each at least 0.
    pub fn new(given: Num, used: Num) -> Self {
        debug_assert!(given >= Num::ZERO, "a power given is at least 0");
        debug_assert!(used >= Num::ZERO, "a power used is at least 0");
        Self { given, used }
    }

    pub const fn given(self) -> Num {
        self.given
    }

    pub const fn used(self) -> Num {
        self.used
    }

    /// Whether its units take more than they give (power.md, Short while it uses more than it
    /// gives).
    pub fn is_short(self) -> bool {
        self.used > self.given
    }
}

impl SimComponent for PlayerPower {
    const NAME: &'static str = "production.power";
    type Host = Player;

    // Derived, so no snapshot holds one for a restore to check.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for PlayerPower {
    const KIND: DataKind = DataKind::Supply;
    type Sending = SentOnChange;
}
