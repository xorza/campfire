use std::fmt::Debug;
use std::marker::PhantomData;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::{Audience, StateValue};
use campfire_sim::SimComponent;
use campfire_states::{NotSent, Replication, Sending, SentOnChange};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;
use crate::unit_state_book::UnitStateBook;

/// The fields of a unit's script state whose `audience` is `S`'s: a value for each such field its
/// type declares, in the order of their names. A unit whose type declares none has none, so each
/// class is a component of its own kind, which its own clients receive
/// (10-replication-audiences.md, One kind for each type).
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitState<S: StateAudience> {
    values: Vec<StateValue>,
    #[serde(skip)]
    audience: PhantomData<S>,
}

/// A class of script state fields by their `audience`: the clients that receive them, as their
/// kind.
pub trait StateAudience: Debug + Clone + PartialEq + Eq + Send + Sync + 'static {
    const AUDIENCE: Audience;
    const NAME: &'static str;
    const KIND: DataKind;
    type Sending: Sending<UnitState<Self>>;
}

/// What runs with the class type of a `Audience` that only a value names: `on` is the one place a
/// class's value picks its type.
pub(crate) trait AudienceCall: Sized {
    type Output;

    fn call<S: StateAudience>(self) -> Self::Output;

    /// Runs `call` with the class type of `audience`.
    fn on(self, audience: Audience) -> Self::Output {
        match audience {
            Audience::None => self.call::<AudienceNone>(),
            Audience::Owner => self.call::<AudienceOwner>(),
            Audience::Team => self.call::<AudienceTeam>(),
            Audience::All => self.call::<AudienceAll>(),
        }
    }
}

/// The fields of `audience = "none"`, the default, which no client receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceNone;

/// The fields of `audience = "owner"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceOwner;

/// The fields of `audience = "team"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceTeam;

/// The fields of `audience = "all"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceAll;

impl StateAudience for AudienceNone {
    const AUDIENCE: Audience = Audience::None;
    const NAME: &'static str = "units.state";
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

impl StateAudience for AudienceOwner {
    const AUDIENCE: Audience = Audience::Owner;
    const NAME: &'static str = "units.state_owner";
    const KIND: DataKind = DataKind::UnitStateOwner;
    type Sending = SentOnChange;
}

impl StateAudience for AudienceTeam {
    const AUDIENCE: Audience = Audience::Team;
    const NAME: &'static str = "units.state_team";
    const KIND: DataKind = DataKind::UnitStateTeam;
    type Sending = SentOnChange;
}

impl StateAudience for AudienceAll {
    const AUDIENCE: Audience = Audience::All;
    const NAME: &'static str = "units.state_all";
    const KIND: DataKind = DataKind::UnitStateAll;
    type Sending = SentOnChange;
}

/// The kind of a unit's state fields of an audience, as its class's component sends them.
pub(crate) struct KindOf;

impl AudienceCall for KindOf {
    type Output = DataKind;

    fn call<S: StateAudience>(self) -> DataKind {
        S::KIND
    }
}

impl<S: StateAudience> UnitState<S> {
    pub const fn new(values: Vec<StateValue>) -> Self {
        Self {
            values,
            audience: PhantomData,
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

impl<S: StateAudience> SimComponent for UnitState<S> {
    const NAME: &'static str = S::NAME;
    type Host = UnitEntity;

    // A field its type does not declare in the class, or a value of another type, reaches a
    // script as a value it never wrote, or is read past the type's fields.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<UnitStateBook>();
        world
            .get::<UnitType>(entity)
            .is_some_and(|&unit_type| book.holds(unit_type, S::AUDIENCE, &self.values))
    }
}

impl<S: StateAudience> Replication for UnitState<S> {
    const KIND: DataKind = S::KIND;
    type Sending = S::Sending;
}
