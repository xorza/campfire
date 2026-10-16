use std::fmt::Debug;
use std::marker::PhantomData;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::{StateValue, SyncTo};
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication, Sending, SentOnChange};
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;
use crate::unit_state_book::UnitStateBook;

/// The fields of a unit's script state whose `sync` is `S`'s: a value for each such field its type
/// declares, in the order of their names. A unit whose type declares none has none, so each class
/// is a component of its own kind, which its own clients receive (14-replication-audiences.md, One
/// kind for each type).
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitState<S: StateSync> {
    values: Vec<StateValue>,
    #[serde(skip)]
    sync: PhantomData<S>,
}

/// A class of script state fields by their `sync`: the clients that receive them, as their kind.
pub trait StateSync: Debug + Clone + PartialEq + Eq + Send + Sync + 'static {
    const SYNC: SyncTo;
    const NAME: &'static str;
    const KIND: DataKind;
    type Sending: Sending<UnitState<Self>>;
}

/// What runs with the class type of a `SyncTo` that only a value names: `on` is the one place a
/// class's value picks its type.
pub(crate) trait SyncCall: Sized {
    type Output;

    fn call<S: StateSync>(self) -> Self::Output;

    /// Runs `call` with the class type of `sync`.
    fn on(self, sync: SyncTo) -> Self::Output {
        match sync {
            SyncTo::None => self.call::<SyncNone>(),
            SyncTo::Owner => self.call::<SyncOwner>(),
            SyncTo::Team => self.call::<SyncTeam>(),
            SyncTo::All => self.call::<SyncAll>(),
        }
    }
}

/// The fields of `sync = "none"`, the default, which no client receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncNone;

/// The fields of `sync = "owner"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOwner;

/// The fields of `sync = "team"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTeam;

/// The fields of `sync = "all"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncAll;

impl StateSync for SyncNone {
    const SYNC: SyncTo = SyncTo::None;
    const NAME: &'static str = "units.state";
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

impl StateSync for SyncOwner {
    const SYNC: SyncTo = SyncTo::Owner;
    const NAME: &'static str = "units.state_owner";
    const KIND: DataKind = DataKind::UnitStateOwner;
    type Sending = SentOnChange;
}

impl StateSync for SyncTeam {
    const SYNC: SyncTo = SyncTo::Team;
    const NAME: &'static str = "units.state_team";
    const KIND: DataKind = DataKind::UnitStateTeam;
    type Sending = SentOnChange;
}

impl StateSync for SyncAll {
    const SYNC: SyncTo = SyncTo::All;
    const NAME: &'static str = "units.state_all";
    const KIND: DataKind = DataKind::UnitStateAll;
    type Sending = SentOnChange;
}

impl<S: StateSync> UnitState<S> {
    pub const fn new(values: Vec<StateValue>) -> Self {
        Self {
            values,
            sync: PhantomData,
        }
    }

    pub const fn values(&self) -> &[StateValue] {
        self.values.as_slice()
    }

    /// Sets the field at `at` among its class's, of a value of its type, as a call's write.
    pub fn set(&mut self, at: usize, value: StateValue) {
        debug_assert_eq!(
            self.values[at].kind(),
            value.kind(),
            "a value of the field's type"
        );
        self.values[at] = value;
    }
}

impl<S: StateSync> SimComponent for UnitState<S> {
    const NAME: &'static str = S::NAME;
    type Host = UnitEntity;

    // A field its type does not declare in the class, or a value of another type, reaches a
    // script as a value it never wrote, or is read past the type's fields.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<UnitStateBook>();
        world
            .get::<UnitType>(entity)
            .is_some_and(|&unit_type| book.holds(unit_type, S::SYNC, &self.values))
    }
}

impl<S: StateSync> Replication for UnitState<S> {
    const KIND: DataKind = S::KIND;
    type Sending = S::Sending;
}
