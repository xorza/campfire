use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::scripts::state_value::StateValue;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_type::UnitType;

/// A unit's script state: a value for each field its type declares, in the order of their names.
/// A unit whose type declares none has none.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct UnitState(Vec<StateValue>);

impl UnitState {
    pub(crate) const fn new(values: Vec<StateValue>) -> UnitState {
        UnitState(values)
    }

    pub(crate) const fn values(&self) -> &[StateValue] {
        self.0.as_slice()
    }

    /// Sets the field at `at`, of a value of its type, as a call's write.
    pub(crate) fn set(&mut self, at: usize, value: StateValue) {
        debug_assert_eq!(
            self.0[at].kind(),
            value.kind(),
            "a value of the field's type"
        );
        self.0[at] = value;
    }
}

impl SimComponent for UnitState {
    const NAME: &'static str = "units.state";

    // A field its type does not declare, or a value of another type, reaches a script as a value
    // it never wrote, or is read past the type's fields.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.get_resource::<UnitStateBook>();
        let unit_type = world.get::<UnitType>(entity);
        book.zip(unit_type)
            .is_some_and(|(book, &unit_type)| book.holds(unit_type, &self.0))
    }
}
