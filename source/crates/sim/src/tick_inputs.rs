use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_protocol::PlayerSlot;

/// The player inputs applied in the running tick. The runner fills it from the session log before
/// each tick, and the schedule empties it when the tick ends, so no tick sees another's inputs.
/// It is not state: the log holds the inputs.
#[derive(Resource, Debug, Default)]
pub struct TickInputs {
    inputs: Vec<Entry>,
    payloads: Vec<u8>,
}

/// One input: the player's slot, and the payload, a list of commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickInput<'a> {
    pub slot: PlayerSlot,
    pub payload: &'a [u8],
}

#[derive(Debug)]
struct Entry {
    slot: PlayerSlot,
    payload: Range<usize>,
}

impl TickInputs {
    pub fn push(&mut self, input: TickInput<'_>) {
        let start = self.payloads.len();
        self.payloads.extend_from_slice(input.payload);
        self.inputs.push(Entry {
            slot: input.slot,
            payload: start..self.payloads.len(),
        });
    }

    /// The inputs in the order they were pushed.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = TickInput<'_>> {
        self.inputs.iter().map(|entry| TickInput {
            slot: entry.slot,
            payload: &self.payloads[entry.payload.clone()],
        })
    }

    pub(crate) fn clear(&mut self) {
        self.inputs.clear();
        self.payloads.clear();
    }
}
