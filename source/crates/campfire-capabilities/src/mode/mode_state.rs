use std::marker::PhantomData;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::mode::mode_book::ModeBook;
use crate::mode::mode_data::ModeData;
use crate::players::match_entity::MatchEntity;
use crate::scripts::ctx::Ctx;
use crate::scripts::state_value::StateValue;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::{NotSent, Sending, SentOnChange};
use crate::units::unit_state::{StateSync, SyncAll, SyncNone};

/// The fields of the mode's script state whose `sync` is `S`'s, on the match's entity: a value
/// for each such field its data declares, in the order of their names. Each class is a component
/// of its own kind (design 14, D3); a mode field syncs to no owner or team, which the match lacks
/// (D7).
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeState<S: ModeSync> {
    values: Vec<StateValue>,
    #[serde(skip)]
    sync: PhantomData<S>,
}

/// A class of the mode's state fields by their `sync`: `none` or `all`.
pub trait ModeSync: StateSync {
    const MODE_NAME: &'static str;
    const MODE_KIND: DataKind;
    type ModeSending: Sending<ModeState<Self>>;
}

impl ModeSync for SyncNone {
    const MODE_NAME: &'static str = "mode.state";
    const MODE_KIND: DataKind = DataKind::Server;
    type ModeSending = NotSent;
}

impl ModeSync for SyncAll {
    const MODE_NAME: &'static str = "mode.state_all";
    const MODE_KIND: DataKind = DataKind::ModeState;
    type ModeSending = SentOnChange;
}

impl<S: ModeSync> ModeState<S> {
    pub(crate) const fn new(values: Vec<StateValue>) -> ModeState<S> {
        ModeState {
            values,
            sync: PhantomData,
        }
    }

    pub const fn values(&self) -> &[StateValue] {
        self.values.as_slice()
    }

    /// The fields of the class `data` declares, each at its first value, in the order of their
    /// names.
    pub(crate) fn initial(data: &ModeData) -> ModeState<S> {
        let fields = data.state.values().filter(|field| field.sync == S::SYNC);
        ModeState::new(fields.map(|field| field.decl.initial().clone()).collect())
    }

    /// The class's fields in `world`, on the match's entity, which every match with a mode spawns
    /// with them.
    pub(crate) fn of(world: &World) -> &[StateValue] {
        let entity = MatchEntity::entity(world).expect("a match that started has its entity");
        let state = world.get::<ModeState<S>>(entity);
        &state
            .expect("the match's entity holds the mode's state")
            .values
    }

    /// Writes `values`, the class's fields as a call left them, in `world`, unless they are what
    /// it holds, so a call that changed none marks nothing changed.
    pub(crate) fn commit(world: &mut World, values: &[StateValue]) {
        let entity = MatchEntity::entity(world).expect("a match that started has its entity");
        let mut state = world
            .get_mut::<ModeState<S>>(entity)
            .expect("the match's entity holds the mode's state");
        if state.values != values {
            state.values.clone_from_slice(values);
        }
    }
}

impl<S: ModeSync> SimComponent for ModeState<S> {
    const NAME: &'static str = S::MODE_NAME;
    type Host = MatchEntity;

    // A field the class does not declare, or of another type, reaches its script as a value it
    // never wrote.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.get_non_send::<Ctx>().and_then(ModeBook::of);
        book.is_none_or(|book| book.schema.fits_state(S::SYNC, &self.values))
    }
}

impl<S: ModeSync> Replication for ModeState<S> {
    const KIND: DataKind = S::MODE_KIND;
    type Sending = S::ModeSending;
}
