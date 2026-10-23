use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_common::Tick;

/// The inputs the player sent, to predict with again after a rollback, each in the tick it takes
/// effect in: from the oldest a rollback can reach on. Inputs are kept in the order of their seq,
/// and their ticks never decrease, so a tick's inputs are one run, and their payloads sit end to
/// end in one buffer.
#[derive(Resource, Debug, Default)]
pub struct SentInputs {
    inputs: Vec<SentInput>,
    payloads: Vec<u8>,
    /// The body of the order being written, kept so no order allocates.
    body: Vec<u8>,
}

#[derive(Debug)]
struct SentInput {
    seq: u64,
    stamp: Tick,
    /// The tick it takes effect in, as the server acknowledged it, or as the client expects it
    /// to: its stamp, or later, behind the input before it, as the server applies a player's
    /// inputs in order.
    tick: Tick,
    landing: Landing,
    payload: Range<usize>,
}

/// What the client knows of where an input takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    /// The server has not acknowledged it yet.
    Expected,
    /// The server applies it in its `tick`.
    Applied,
    /// The server logged it early, and it never takes effect.
    Never,
}

#[expect(
    clippy::len_without_is_empty,
    reason = "the client reads the count to number what it sends, never whether it holds any"
)]
impl SentInputs {
    /// How many inputs it holds.
    pub const fn len(&self) -> usize {
        self.inputs.len()
    }

    /// The payloads that take effect in `tick`.
    pub fn at(&self, tick: Tick) -> impl Iterator<Item = &[u8]> {
        let start = self.inputs.partition_point(|input| input.tick < tick);
        self.inputs[start..]
            .iter()
            .take_while(move |input| input.tick == tick)
            .filter(|input| input.landing != Landing::Never)
            .map(|input| &self.payloads[input.payload.clone()])
    }

    /// The payloads of the inputs from the `first` on.
    pub fn since(&self, first: usize) -> impl ExactSizeIterator<Item = &[u8]> + Clone {
        self.inputs[first..]
            .iter()
            .map(|input| &self.payloads[input.payload.clone()])
    }

    /// Keeps the payload `write` appends, with a body buffer it may use, as the input of `seq`
    /// stamped `stamp`. The payload holds `max_payload_len` bytes at most, as the terms the client
    /// joined hold every input the engine makes (05-protocol-spec.md, A session's terms hold every
    /// input): a longer one is a logic error, which the server would refuse and end the link for.
    pub fn push(
        &mut self,
        seq: u64,
        stamp: Tick,
        max_payload_len: u32,
        write: impl FnOnce(&mut Vec<u8>, &mut Vec<u8>),
    ) {
        let last = self.inputs.last();
        debug_assert!(
            last.is_none_or(|last| last.stamp <= stamp && last.seq + 1 == seq),
            "seqs follow each other, and stamps never decrease"
        );
        let tick = last.map_or(stamp, |last| last.tick.max(stamp));
        let start = self.payloads.len();
        write(&mut self.body, &mut self.payloads);
        let len = self.payloads.len() - start;
        assert!(
            u32::try_from(len).is_ok_and(|len| len <= max_payload_len),
            "an input of {len} bytes passes the session's {max_payload_len}"
        );
        self.inputs.push(SentInput {
            seq,
            stamp,
            tick,
            landing: Landing::Expected,
            payload: start..self.payloads.len(),
        });
    }

    /// Takes the server's word on where the inputs from seq `first` on take effect, each in the
    /// tick `applied` gives, or never; and expects each input it has not acknowledged yet behind
    /// them. An input the client no longer keeps, or never sent, is passed over.
    pub fn acknowledge(&mut self, first: u64, applied: &[Option<Tick>]) {
        let Some(oldest) = self.inputs.first().map(|input| input.seq) else {
            return;
        };
        let mut changed = self.inputs.len();
        for (seq, &tick) in (first..).zip(applied) {
            let Some(at) = seq
                .checked_sub(oldest)
                .and_then(|at| usize::try_from(at).ok())
            else {
                continue;
            };
            let Some(input) = self.inputs.get_mut(at) else {
                break;
            };
            input.landing = match tick {
                Some(tick) => {
                    input.tick = tick;
                    Landing::Applied
                }
                None => Landing::Never,
            };
            changed = changed.min(at);
        }
        let mut previous = changed.checked_sub(1).map(|at| self.inputs[at].tick);
        for input in &mut self.inputs[changed..] {
            let behind = previous.map_or(input.stamp, |previous| previous.max(input.stamp));
            input.tick = match input.landing {
                Landing::Applied => {
                    debug_assert!(previous.is_none_or(|previous| previous <= input.tick));
                    input.tick
                }
                Landing::Expected => behind,
                Landing::Never => previous.unwrap_or(input.stamp),
            };
            previous = Some(input.tick);
        }
    }

    /// Drops every input, as a client whose link failed does.
    pub fn clear(&mut self) {
        self.inputs.clear();
        self.payloads.clear();
    }

    /// Drops the inputs that take effect before `oldest`, which no rollback replays.
    pub fn prune(&mut self, oldest: Tick) {
        let cut = self.inputs.partition_point(|input| input.tick < oldest);
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
