use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Seat;
use campfire_players::PlayersWorld;
use campfire_script::{Audience, StateField, StateSchema, StateValue};
use campfire_units::{AudienceAll, AudienceNone, AudienceOwner, AudienceTeam};
use campfire_values::NameId;

use crate::player_state::{PlayerAudience, PlayerState};

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

/// A call's write of `value` to the field `field` of player `seat`.
#[derive(Debug)]
struct PlayerWrite {
    seat: Seat,
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
        for slot in (0..players).map(Seat::new) {
            let entity = Self::entity(world, slot);
            for place in schema.places() {
                let class = match place.audience {
                    Audience::None => Self::class::<AudienceNone>(world, entity),
                    Audience::Owner => Self::class::<AudienceOwner>(world, entity),
                    Audience::Team => Self::class::<AudienceTeam>(world, entity),
                    Audience::All => Self::class::<AudienceAll>(world, entity),
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
        for PlayerWrite { seat, field, value } in self.written.drain(..) {
            self.values[seat.index() * self.fields + field.index()].clone_from(&value);
            let entity = Self::entity(world, seat);
            let place = schema.place(field);
            match place.audience {
                Audience::None => Self::write::<AudienceNone>(world, entity, place.at, value),
                Audience::Owner => Self::write::<AudienceOwner>(world, entity, place.at, value),
                Audience::Team => Self::write::<AudienceTeam>(world, entity, place.at, value),
                Audience::All => Self::write::<AudienceAll>(world, entity, place.at, value),
            }
        }
    }

    /// The field `field` of player `seat`, as the call last wrote it, or as the batch holds it.
    pub(crate) fn get(&self, seat: Seat, field: StateField) -> &StateValue {
        let mut written = self.written.iter().rev();
        let found = written.find(|write| write.seat == seat && write.field == field);
        found.map_or_else(
            || &self.values[seat.index() * self.fields + field.index()],
            |write| &write.value,
        )
    }

    /// Writes `value`, a value of `field`, to the field of player `seat`, for the call to read
    /// back and to apply when it ends.
    pub(crate) fn set(&mut self, seat: Seat, field: StateField, value: StateValue) {
        self.written.push(PlayerWrite { seat, field, value });
    }

    fn entity(world: &World, seat: Seat) -> Entity {
        world
            .player_entity(seat)
            .expect("a match that started has each player's entity")
    }

    fn class<S: PlayerAudience>(world: &World, entity: Entity) -> &[StateValue] {
        world
            .get::<PlayerState<S>>(entity)
            .expect("each player's entity holds each class of its state")
            .values()
    }

    /// Writes `value` to the field at `at` of class `S`, unless it holds it, as a write marks it
    /// changed.
    fn write<S: PlayerAudience>(world: &mut World, entity: Entity, at: usize, value: StateValue) {
        let mut state = world
            .get_mut::<PlayerState<S>>(entity)
            .expect("each player's entity holds each class of its state");
        if state.values()[at] != value {
            state.set(at, value);
        }
    }
}
