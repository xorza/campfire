use std::marker::PhantomData;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::MatchEntity;
use campfire_script::StateValue;
use campfire_sim::SimComponent;
use campfire_states::{NotSent, Replication, Sending, SentOnChange};
use campfire_units::{AudienceAll, AudienceNone, StateAudience};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::match_mode::MatchMode;
use crate::mode_schema::ModeSchema;

/// The fields of the mode's script state whose `audience` is `S`'s, on the match's entity: a value
/// for each such field its data declares, in the order of their names. Each class is a component of
/// its own kind (10-replication-audiences.md, One kind for each type); a mode field goes to no
/// owner or team, which the match lacks (D7).
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeState<S: ModeAudience> {
    values: Vec<StateValue>,
    #[serde(skip)]
    audience: PhantomData<S>,
}

/// A class of the mode's state fields by their `audience`: `none` or `all`.
pub trait ModeAudience: StateAudience {
    const MODE_NAME: &'static str;
    const MODE_KIND: DataKind;
    type ModeSending: Sending<ModeState<Self>>;
}

impl ModeAudience for AudienceNone {
    const MODE_NAME: &'static str = "mode.state";
    const MODE_KIND: DataKind = DataKind::Server;
    type ModeSending = NotSent;
}

impl ModeAudience for AudienceAll {
    const MODE_NAME: &'static str = "mode.state_all";
    const MODE_KIND: DataKind = DataKind::ModeState;
    type ModeSending = SentOnChange;
}

impl<S: ModeAudience> ModeState<S> {
    pub const fn new(values: Vec<StateValue>) -> Self {
        Self {
            values,
            audience: PhantomData,
        }
    }

    pub const fn values(&self) -> &[StateValue] {
        self.values.as_slice()
    }

    pub(crate) const fn values_mut(&mut self) -> &mut [StateValue] {
        self.values.as_mut_slice()
    }

    /// The fields of its class that `schema` declares, each at its first value, in the order of
    /// their names.
    pub(crate) fn initial(schema: &ModeSchema) -> Self {
        Self::new(schema.state.initial_of(S::AUDIENCE).cloned().collect())
    }
}

impl<S: ModeAudience> SimComponent for ModeState<S> {
    const NAME: &'static str = S::MODE_NAME;
    type Host = MatchEntity;

    // A field the class does not declare, or of another type, reaches its script as a value it
    // never wrote.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<MatchMode>();
        book.schema.state.holds(S::AUDIENCE, &self.values)
    }
}

impl<S: ModeAudience> Replication for ModeState<S> {
    const KIND: DataKind = S::MODE_KIND;
    type Sending = S::ModeSending;
}
