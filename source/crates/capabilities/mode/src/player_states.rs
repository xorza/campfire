use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_players::PlayersWorld;
use campfire_script::{StateField, StateSchema, StateValue, SyncTo};
use campfire_units::{SyncAll, SyncNone, SyncOwner, SyncTeam};
use campfire_values::NameId;

use crate::player_state::{PlayerState, PlayerSync};

/// The players' script state as a call sees it: each player's fields in turn, every field in the
/// order of their names, read once a batch, as each call of the batch commits its writes into
/// them; and the running call's writes, which commit with the mode's part of the frame, so a
/// failed call writes nothing.
#[derive(Debug, Default)]
pub(crate) struct PlayerStates {
    values: Vec<StateValue>,
    /// How many fields each player holds.
    fields: usize,
    /// The running call's writes, in order.
    written: Vec<PlayerWrite>,
}

/// A call's write of `value` to the field `field` of player `slot`.
#[derive(Debug)]
struct PlayerWrite {
    slot: PlayerSlot,
    field: StateField,
    value: StateValue,
}

impl PlayerStates {
    /// Reads the fields of `schema` of each of `players` players as `world` holds them.
    pub(crate) fn read(&mut self, world: &World, schema: &StateSchema, players: u32) {
        self.values.clear();
        self.fields = schema.len();
        if schema.is_empty() {
            return;
        }
        let count = usize::try_from(players).expect("players fit usize");
        self.values.reserve_exact(schema.len() * count);
        for slot in (0..players).map(PlayerSlot::new) {
            let entity = Self::entity(world, slot);
            for place in schema.places() {
                let class = match place.sync {
                    SyncTo::None => Self::class::<SyncNone>(world, entity),
                    SyncTo::Owner => Self::class::<SyncOwner>(world, entity),
                    SyncTo::Team => Self::class::<SyncTeam>(world, entity),
                    SyncTo::All => Self::class::<SyncAll>(world, entity),
                };
                self.values.push(class[place.at].clone());
            }
        }
    }

    /// Drops the writes of the call before, which did not commit.
    pub(crate) fn begin(&mut self) {
        self.written.clear();
    }

    /// Writes the call's writes to the players' entities, each class's field at its place in
    /// `schema`, and keeps them for the batch's next calls.
    pub(crate) fn commit(&mut self, world: &mut World, schema: &StateSchema) {
        for PlayerWrite { slot, field, value } in self.written.drain(..) {
            self.values[slot.index() * self.fields + field.index()].clone_from(&value);
            let entity = Self::entity(world, slot);
            let place = schema.place(field);
            match place.sync {
                SyncTo::None => Self::write::<SyncNone>(world, entity, place.at, value),
                SyncTo::Owner => Self::write::<SyncOwner>(world, entity, place.at, value),
                SyncTo::Team => Self::write::<SyncTeam>(world, entity, place.at, value),
                SyncTo::All => Self::write::<SyncAll>(world, entity, place.at, value),
            }
        }
    }

    /// The field `field` of player `slot`, as the call last wrote it, or as the batch holds it.
    pub(crate) fn get(&self, slot: PlayerSlot, field: StateField) -> &StateValue {
        let mut written = self.written.iter().rev();
        let found = written.find(|write| write.slot == slot && write.field == field);
        found.map_or_else(
            || &self.values[slot.index() * self.fields + field.index()],
            |write| &write.value,
        )
    }

    /// Writes `value`, a value of `field`, to the field of player `slot`, for the call to read
    /// back and to apply when it ends.
    pub(crate) fn set(&mut self, slot: PlayerSlot, field: StateField, value: StateValue) {
        self.written.push(PlayerWrite { slot, field, value });
    }

    fn entity(world: &World, slot: PlayerSlot) -> Entity {
        world
            .player_entity(slot)
            .expect("a match that started has each player's entity")
    }

    fn class<S: PlayerSync>(world: &World, entity: Entity) -> &[StateValue] {
        world
            .get::<PlayerState<S>>(entity)
            .expect("each player's entity holds each class of its state")
            .values()
    }

    /// Writes `value` to the field at `at` of class `S`, unless it holds it, as a write marks it
    /// changed.
    fn write<S: PlayerSync>(world: &mut World, entity: Entity, at: usize, value: StateValue) {
        let mut state = world
            .get_mut::<PlayerState<S>>(entity)
            .expect("each player's entity holds each class of its state");
        if state.values()[at] != value {
            state.set(at, value);
        }
    }
}
