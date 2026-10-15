use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::{StatePlace, StateValue, SyncTo};

use campfire_values::UnitType;

use crate::units::spawn_target::SpawnTarget;
use crate::units::unit_state::{StateSync, SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
use crate::units::unit_state_book::UnitStateBook;

/// A new unit's script state: the values of each class of its fields, in the order of their
/// names, each class a `UnitState` of its own as the unit spawns.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UnitStates {
    none: Vec<StateValue>,
    owner: Vec<StateValue>,
    team: Vec<StateValue>,
    all: Vec<StateValue>,
}

impl UnitStates {
    /// The state a unit of `unit_type` spawns with, by `book`, each field at its first value.
    pub fn initial(book: &UnitStateBook, unit_type: UnitType) -> UnitStates {
        let mut states = UnitStates::default();
        if let Some(schema) = book.schema(unit_type) {
            for (value, place) in schema.initial().iter().zip(schema.places()) {
                states.push(place.sync, value.clone());
            }
        }
        states
    }

    const fn class_mut(&mut self, sync: SyncTo) -> &mut Vec<StateValue> {
        match sync {
            SyncTo::None => &mut self.none,
            SyncTo::Owner => &mut self.owner,
            SyncTo::Team => &mut self.team,
            SyncTo::All => &mut self.all,
        }
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

    /// Inserts a component into `target` for each class it holds a field of.
    pub fn insert_into(self, target: &mut impl SpawnTarget) {
        let UnitStates {
            none,
            owner,
            team,
            all,
        } = self;
        UnitStates::insert_class::<SyncNone>(target, none);
        UnitStates::insert_class::<SyncOwner>(target, owner);
        UnitStates::insert_class::<SyncTeam>(target, team);
        UnitStates::insert_class::<SyncAll>(target, all);
    }

    fn insert_class<S: StateSync>(
        target: &mut impl SpawnTarget,
        values: impl IntoIterator<Item = StateValue>,
    ) {
        let values: Vec<StateValue> = values.into_iter().collect();
        if !values.is_empty() {
            target.insert_part(UnitState::<S>::new(values));
        }
    }

    fn write_class<S: StateSync>(world: &mut World, entity: Entity, at: usize, value: StateValue) {
        let mut state = world
            .get_mut::<UnitState<S>>(entity)
            .expect("a unit holds the class of each field its type declares");
        state.set(at, value);
    }

    /// Writes `value` to the field at `place` of the unit of `entity`, which holds the field's
    /// class, as a unit holds each class its type declares a field of.
    pub fn write(world: &mut World, entity: Entity, place: StatePlace, value: StateValue) {
        match place.sync {
            SyncTo::None => UnitStates::write_class::<SyncNone>(world, entity, place.at, value),
            SyncTo::Owner => UnitStates::write_class::<SyncOwner>(world, entity, place.at, value),
            SyncTo::Team => UnitStates::write_class::<SyncTeam>(world, entity, place.at, value),
            SyncTo::All => UnitStates::write_class::<SyncAll>(world, entity, place.at, value),
        }
    }
}
