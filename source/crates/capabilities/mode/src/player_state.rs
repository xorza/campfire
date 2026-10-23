use std::marker::PhantomData;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_script::{Audience, StateSchema, StateValue};
use campfire_sim::SimComponent;
use campfire_states::{NotSent, Replication, Sending, SentOnChange};
use campfire_units::{AudienceAll, AudienceNone, AudienceOwner, AudienceTeam, StateAudience};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::match_mode::MatchMode;

/// The fields of a player's script state whose `audience` is `S`'s, on its entity: a value for each
/// such field the mode's `[player_state]` declares, in the order of their names. Each class is a
/// component of its own kind (10-replication-audiences.md, One kind for each type); a player's
/// entity is its own owner's.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerState<S: PlayerAudience> {
    values: Vec<StateValue>,
    #[serde(skip)]
    audience: PhantomData<S>,
}

/// The kind of the players' state fields of `audience`, as its class's component sends them.
pub(crate) const fn kind_of(audience: Audience) -> DataKind {
    match audience {
        Audience::None => AudienceNone::PLAYER_KIND,
        Audience::Owner => AudienceOwner::PLAYER_KIND,
        Audience::Team => AudienceTeam::PLAYER_KIND,
        Audience::All => AudienceAll::PLAYER_KIND,
    }
}

/// A class of the players' state fields by their `audience`.
pub trait PlayerAudience: StateAudience {
    const PLAYER_NAME: &'static str;
    const PLAYER_KIND: DataKind;
    type PlayerSending: Sending<PlayerState<Self>>;
}

impl PlayerAudience for AudienceNone {
    const PLAYER_NAME: &'static str = "mode.player_state";
    const PLAYER_KIND: DataKind = DataKind::Server;
    type PlayerSending = NotSent;
}

impl PlayerAudience for AudienceOwner {
    const PLAYER_NAME: &'static str = "mode.player_state_owner";
    const PLAYER_KIND: DataKind = DataKind::PlayerStateOwner;
    type PlayerSending = SentOnChange;
}

impl PlayerAudience for AudienceTeam {
    const PLAYER_NAME: &'static str = "mode.player_state_team";
    const PLAYER_KIND: DataKind = DataKind::PlayerStateTeam;
    type PlayerSending = SentOnChange;
}

impl PlayerAudience for AudienceAll {
    const PLAYER_NAME: &'static str = "mode.player_state_all";
    const PLAYER_KIND: DataKind = DataKind::PlayerStateAll;
    type PlayerSending = SentOnChange;
}

impl<S: PlayerAudience> PlayerState<S> {
    pub const fn new(values: Vec<StateValue>) -> Self {
        Self {
            values,
            audience: PhantomData,
        }
    }

    pub const fn values(&self) -> &[StateValue] {
        self.values.as_slice()
    }

    /// Sets the field at `at` among its class's, of a value of its field, as a call's write.
    pub(crate) fn set(&mut self, at: usize, value: StateValue) {
        self.values[at] = value;
    }

    /// The fields of its class that `schema` declares, each at its first value, in the order of
    /// their names.
    pub(crate) fn initial(schema: &StateSchema) -> Self {
        Self::new(schema.initial_in(S::AUDIENCE).cloned().collect())
    }
}

impl<S: PlayerAudience> SimComponent for PlayerState<S> {
    const NAME: &'static str = S::PLAYER_NAME;
    type Host = Player;

    // A field the class does not declare, or a value no value of its field, reaches a script as
    // a value it never wrote.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<MatchMode>();
        book.schema.players.holds(S::AUDIENCE, &self.values)
    }
}

impl<S: PlayerAudience> Replication for PlayerState<S> {
    const KIND: DataKind = S::PLAYER_KIND;
    type Sending = S::PlayerSending;
}
