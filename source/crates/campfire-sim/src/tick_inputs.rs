use std::ops::Range;

use bevy_ecs::resource::Resource;

use campfire_common::PlayerSlot;

use crate::capability::Capability;
use crate::command::Command;
use crate::slot_event::SlotEvent;

/// The inputs applied in the running tick: the players' inputs, and the changes of who controls
/// a slot. The runner fills it from the session log before each tick, and the schedule empties
/// it when the tick ends, so no tick sees another's inputs. It is not state: the log holds the
/// inputs. Each input's commands are read once, as it comes.
#[derive(Resource, Debug, Default)]
pub struct TickInputs {
    inputs: Vec<Entry>,
    payloads: Vec<u8>,
    /// The commands of the inputs, in order: none of an input whose payload has a flaw.
    commands: Vec<Stored>,
    /// The slot events, in the order the log took them.
    slot_events: Vec<SlotEvent>,
}

/// A command of a player's input, as the capability it goes to reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerCommand<'a> {
    pub slot: PlayerSlot,
    pub body: &'a [u8],
}

/// A command held: its player, its capability and its body among the payloads.
#[derive(Debug)]
struct Stored {
    slot: PlayerSlot,
    capability: Capability,
    body: Range<usize>,
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
        let held = self.commands.len();
        let payload = &self.payloads[start..];
        let commands = &mut self.commands;
        let whole = Command::read(payload, |command, body| {
            commands.push(Stored {
                slot: input.slot,
                capability: command.capability,
                body: start + body.start..start + body.end,
            });
        });
        if !whole {
            self.commands.truncate(held);
        }
    }

    /// The commands to `capability` of the inputs, in the order of the inputs and of each
    /// input's commands.
    pub fn commands(&self, capability: Capability) -> impl Iterator<Item = PlayerCommand<'_>> {
        let ours = self
            .commands
            .iter()
            .filter(move |held| held.capability == capability);
        ours.map(|held| PlayerCommand {
            slot: held.slot,
            body: &self.payloads[held.body.clone()],
        })
    }

    pub fn push_slot_event(&mut self, event: SlotEvent) {
        self.slot_events.push(event);
    }

    /// The slot events in the order they were pushed.
    pub fn slot_events(&self) -> &[SlotEvent] {
        &self.slot_events
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
        self.commands.clear();
        self.slot_events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot_event::SlotEventKind;

    /// The slot and the body of each command to `capability` that `inputs` holds.
    fn read(inputs: &TickInputs, capability: Capability) -> Vec<(u32, &[u8])> {
        let commands = inputs.commands(capability);
        commands
            .map(|command| (command.slot.get(), command.body))
            .collect()
    }

    #[test]
    fn each_input_gives_its_commands_once_and_a_flawed_one_none() {
        let order = |body| Command {
            capability: Capability::Orders,
            body,
        };
        let mode = Command {
            capability: Capability::Mode,
            body: b"m",
        };
        // An empty body among the first's commands takes its place as the others do.
        let first = Command::encode(&[order(b"ab"), mode, order(b""), order(b"c")]);
        // The second's last command is cut short: none of its commands count.
        let whole = Command::encode(&[order(b"x"), order(b"yz")]);
        let cut = &whole[..whole.len() - 1];
        let third = Command::encode(&[order(b"d")]);
        let mut inputs = TickInputs::default();
        for (slot, payload) in [(1, first.as_slice()), (2, cut), (3, &third)] {
            let slot = PlayerSlot::new(slot);
            inputs.push(TickInput { slot, payload });
        }
        assert_eq!(
            read(&inputs, Capability::Orders),
            [(1, &b"ab"[..]), (1, b""), (1, b"c"), (3, b"d")]
        );
        assert_eq!(read(&inputs, Capability::Mode), [(1, &b"m"[..])]);
        assert_eq!(inputs.iter().len(), 3);
        let joined = SlotEvent {
            slot: PlayerSlot::new(2),
            kind: SlotEventKind::Joined,
        };
        inputs.push_slot_event(joined);
        assert_eq!(inputs.slot_events(), [joined]);
        inputs.clear();
        assert_eq!(read(&inputs, Capability::Orders), []);
        assert_eq!(inputs.slot_events(), []);
    }
}
