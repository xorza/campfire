use std::fmt;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::PartWriter;
use campfire_script::{StatePlace, StateValue, SyncTo};
use campfire_values::UnitType;

use crate::unit_state::{StateSync, SyncCall, UnitState};
use crate::unit_state_book::UnitStateBook;

/// A new unit's script state: the values of each class of its fields, in the order of their
/// names, by the class's place, each class a `UnitState` of its own as the unit spawns.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UnitStates([Vec<StateValue>; SyncTo::ALL.len()]);

/// Adds a class's values to a spawning unit's write as its `UnitState`, unless it has none.
struct InsertClass<'t, 'w, 's> {
    target: &'t mut PartWriter<'w, 's>,
    values: Vec<StateValue>,
}

/// Writes the field at `at` of a class of the unit of `entity`.
#[derive(Debug)]
struct WriteClass<'w> {
    world: &'w mut World,
    entity: Entity,
    at: usize,
    value: StateValue,
}

impl UnitStates {
    /// The state a unit of `unit_type` spawns with, by `book`, each field at its first value.
    pub fn initial(book: &UnitStateBook, unit_type: UnitType) -> Self {
        let mut states = Self::default();
        if let Some(schema) = book.schema(unit_type) {
            for (value, place) in schema.initial().iter().zip(schema.places()) {
                states.push(place.sync, value.clone());
            }
        }
        states
    }

    const fn class_mut(&mut self, sync: SyncTo) -> &mut Vec<StateValue> {
        &mut self.0[sync.index()]
    }

    /// Adds `value`, the next field of class `sync`.
    pub fn push(&mut self, sync: SyncTo, value: StateValue) {
        self.class_mut(sync).push(value);
    }

    /// Sets the field at `place`, of a value of its type, as a call's write.
    pub fn set(&mut self, place: StatePlace, value: StateValue) {
        let held = &mut self.class_mut(place.sync)[place.at];
        debug_assert_eq!(held.kind(), value.kind(), "a value of the field's type");
        *held = value;
    }

    /// Adds a component to `target`, a spawning unit's write, for each class it holds a field
    /// of.
    pub fn push_into(self, target: &mut PartWriter<'_, '_>) {
        for (sync, values) in SyncTo::ALL.into_iter().zip(self.0) {
            InsertClass { target, values }.on(sync);
        }
    }

    /// Writes `value` to the field at `place` of the unit of `entity`, which holds the field's
    /// class, as a unit holds each class its type declares a field of.
    pub fn write(world: &mut World, entity: Entity, place: StatePlace, value: StateValue) {
        let at = place.at;
        WriteClass {
            world,
            entity,
            at,
            value,
        }
        .on(place.sync);
    }
}

/// The values it inserts: the unit it inserts them into prints nothing.
impl fmt::Debug for InsertClass<'_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InsertClass")
            .field("values", &self.values)
            .finish_non_exhaustive()
    }
}

impl SyncCall for InsertClass<'_, '_, '_> {
    type Output = ();

    fn call<S: StateSync>(self) {
        if !self.values.is_empty() {
            self.target.push(UnitState::<S>::new(self.values));
        }
    }
}

impl SyncCall for WriteClass<'_> {
    type Output = ();

    fn call<S: StateSync>(self) {
        let mut state = self
            .world
            .get_mut::<UnitState<S>>(self.entity)
            .expect("a unit holds the class of each field its type declares");
        state.set(self.at, self.value);
    }
}
