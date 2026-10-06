use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_capabilities::Order;
use campfire_common::Tick;

/// The inputs the player sent, by stamp, to predict with again after a rollback: from the oldest
/// a rollback can reach on. Stamps never decrease, so a tick's inputs are one run, and their
/// payloads sit end to end in one buffer.
#[derive(Resource, Debug, Default)]
pub(crate) struct SentInputs {
    inputs: Vec<SentInput>,
    payloads: Vec<u8>,
    /// The body of the order being written, kept so no order allocates.
    body: Vec<u8>,
}

#[derive(Debug)]
struct SentInput {
    stamp: Tick,
    payload: Range<usize>,
}

impl SentInputs {
    pub(crate) const fn len(&self) -> usize {
        self.inputs.len()
    }

    /// The payloads stamped `stamp`.
    pub(crate) fn at(&self, stamp: Tick) -> impl Iterator<Item = &[u8]> {
        let start = self.inputs.partition_point(|input| input.stamp < stamp);
        self.inputs[start..]
            .iter()
            .take_while(move |input| input.stamp == stamp)
            .map(|input| &self.payloads[input.payload.clone()])
    }

    /// The payloads of the inputs from the `first` on.
    pub(crate) fn since(&self, first: usize) -> impl ExactSizeIterator<Item = &[u8]> + Clone {
        self.inputs[first..]
            .iter()
            .map(|input| &self.payloads[input.payload.clone()])
    }

    /// Keeps `order` as an input stamped `stamp`; `false`, keeping nothing, when its payload
    /// passes `max_payload_len`.
    pub(crate) fn push(&mut self, stamp: Tick, order: &Order, max_payload_len: u32) -> bool {
        debug_assert!(
            self.inputs.last().is_none_or(|last| last.stamp <= stamp),
            "stamps never decrease"
        );
        let start = self.payloads.len();
        order.write_payload(&mut self.body, &mut self.payloads);
        if self.payloads.len() - start > max_payload_len as usize {
            self.payloads.truncate(start);
            return false;
        }
        self.inputs.push(SentInput {
            stamp,
            payload: start..self.payloads.len(),
        });
        true
    }

    /// Drops every input, as a client whose link failed does.
    pub(crate) fn clear(&mut self) {
        self.inputs.clear();
        self.payloads.clear();
    }

    /// Drops the inputs stamped before `oldest`, which no rollback replays.
    pub(crate) fn prune(&mut self, oldest: Tick) {
        let cut = self.inputs.partition_point(|input| input.stamp < oldest);
        let Some(last) = cut.checked_sub(1).map(|at| &self.inputs[at]) else {
            return;
        };
        let offset = last.payload.end;
        self.inputs.drain(..cut);
        self.payloads.drain(..offset);
        for input in &mut self.inputs {
            input.payload = input.payload.start - offset..input.payload.end - offset;
        }
    }
}

#[cfg(test)]
mod tests;
