use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ops::Range;

use crate::input_hash::InputHash;
use crate::player_input::PlayerInput;
use crate::player_slot::PlayerSlot;
use crate::session_log::error::InputError;

pub(crate) mod error;

/// What the log fixes before the first tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    /// The most ticks an input may land after its stamp; a later one is logged as late.
    pub max_input_delay: u64,
    /// Each player's chain root, by slot: what the player's first input links to.
    pub chain_roots: Vec<InputHash>,
}

/// When a logged input takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    At(u64),
    /// It arrived more than the max input delay after its stamp. It stays in the chain but never
    /// takes effect.
    Late,
}

/// The inputs of a session, in memory, in the order they were logged, and grouped by the tick
/// that was next when each arrived. The applied tick follows from that group, so a log cannot
/// hold a wrong one. The server records inputs as they arrive; a verifier records a published
/// log again, which checks every chain link and gives each tick exactly the server's inputs.
#[derive(Debug)]
pub struct SessionLog {
    header: SessionHeader,
    chains: Vec<Chain>,
    inputs: Vec<LoggedInput>,
    payloads: Vec<u8>,
    /// For each sealed tick, the end of the inputs logged before it ran.
    tick_ends: Vec<u32>,
    /// Inputs applied in a tick not yet sealed, earliest first.
    pending: BinaryHeap<Reverse<Due>>,
    /// Indices of the inputs applied in the tick last sealed.
    due: Vec<u32>,
}

/// A player's chain as logged so far.
#[derive(Debug)]
struct Chain {
    head: InputHash,
    next_seq: u64,
}

#[derive(Debug)]
struct LoggedInput {
    slot: PlayerSlot,
    seq: u64,
    stamp: u64,
    previous: InputHash,
    payload: Range<u32>,
}

/// Orders the inputs of one tick by slot, then seq, whatever order they arrived in, so the host
/// cannot choose who acts first.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Due {
    tick: u64,
    slot: PlayerSlot,
    seq: u64,
    index: u32,
}

impl SessionLog {
    pub fn new(header: SessionHeader) -> SessionLog {
        let chains = header
            .chain_roots
            .iter()
            .map(|&root| Chain {
                head: root,
                next_seq: 0,
            })
            .collect();
        SessionLog {
            header,
            chains,
            inputs: Vec::new(),
            payloads: Vec::new(),
            tick_ends: Vec::new(),
            pending: BinaryHeap::new(),
            due: Vec::new(),
        }
    }

    pub const fn header(&self) -> &SessionHeader {
        &self.header
    }

    /// The tick that the inputs recorded now arrive before.
    pub fn next_tick(&self) -> u64 {
        u64::try_from(self.tick_ends.len()).expect("tick count fits u64")
    }

    /// Logs `input` before the next tick and says when it applies: at `max(stamp, next tick)`,
    /// or late when that is more than the max input delay after the stamp. A refused input
    /// leaves the log unchanged.
    pub fn record(&mut self, input: PlayerInput<'_>) -> Result<Applied, InputError> {
        let chain = self
            .chains
            .get_mut(input.slot.get() as usize)
            .ok_or(InputError::UnknownPlayer)?;
        if input.previous != chain.head {
            return Err(InputError::BrokenLink);
        }
        if input.seq != chain.next_seq {
            return Err(InputError::WrongSeq {
                expected: chain.next_seq,
            });
        }
        chain.head = input.hash();
        chain.next_seq = chain.next_seq.checked_add(1).expect("input seq exhausted");

        let index = offset(self.inputs.len());
        let payload_start = offset(self.payloads.len());
        self.payloads.extend_from_slice(input.payload);
        self.inputs.push(LoggedInput {
            slot: input.slot,
            seq: input.seq,
            stamp: input.stamp,
            previous: input.previous,
            payload: payload_start..offset(self.payloads.len()),
        });

        let next = self.next_tick();
        if next
            .checked_sub(input.stamp)
            .is_some_and(|delay| delay > self.header.max_input_delay)
        {
            return Ok(Applied::Late);
        }
        let tick = input.stamp.max(next);
        self.pending.push(Reverse(Due {
            tick,
            slot: input.slot,
            seq: input.seq,
            index,
        }));
        Ok(Applied::At(tick))
    }

    /// Closes the next tick to new inputs and gives the inputs applied in it, by slot, then seq.
    pub fn seal_tick(&mut self) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        let tick = self.next_tick();
        self.tick_ends.push(offset(self.inputs.len()));
        self.due.clear();
        while let Some(Reverse(due)) = self.pending.peek()
            && due.tick == tick
        {
            self.due.push(due.index);
            self.pending.pop();
        }
        self.due.iter().map(|&index| self.input(index))
    }

    /// The inputs logged before the sealed `tick` ran, in the order they arrived.
    pub fn logged_before(&self, tick: u64) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        let tick = usize::try_from(tick).expect("tick fits usize");
        let start = tick
            .checked_sub(1)
            .map_or(0, |before| self.tick_ends[before]);
        (start..self.tick_ends[tick]).map(|index| self.input(index))
    }

    fn input(&self, index: u32) -> PlayerInput<'_> {
        let logged = &self.inputs[index as usize];
        PlayerInput {
            slot: logged.slot,
            seq: logged.seq,
            stamp: logged.stamp,
            previous: logged.previous,
            payload: &self.payloads[logged.payload.start as usize..logged.payload.end as usize],
        }
    }
}

/// A position in the log's buffers, which stay below 4 GiB.
fn offset(len: usize) -> u32 {
    u32::try_from(len).expect("session log above 4 GiB")
}

#[cfg(test)]
mod tests;
