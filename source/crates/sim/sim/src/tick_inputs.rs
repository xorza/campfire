use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_common::PlayerSlot;

use crate::command::{Addressee, Command};
use crate::player_slot_event::PlayerSlotEvent;

/// The inputs applied in the running tick: the players' inputs, and the changes of who controls a
/// player slot. The runner fills it from the session log before each tick, and the schedule empties
/// it when the tick ends, so no tick sees another's inputs. It is not state: the log holds the
/// inputs. Each input's commands are read once, as it comes.
#[derive(Resource, Debug, Default)]
pub struct TickInputs {
    inputs: Vec<Entry>,
    payloads: Vec<u8>,
    /// The commands of the inputs, in order: none of an input whose payload has a flaw.
    commands: Vec<Stored>,
    /// The player of each input whose payload has a flaw, in order.
    flawed: Vec<PlayerSlot>,
    /// The player slot events, in the order the log took them.
    player_slot_events: Vec<PlayerSlotEvent>,
}

/// A command of a player's input, as what it goes to reads it: its player, its place among every
/// command of the tick, which orders the commands of one player to several addressees, and its
/// body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerCommand<'a> {
    pub player_slot: PlayerSlot,
    pub place: usize,
    pub body: &'a [u8],
}

/// A command held: its player, what it goes to and its body among the payloads.
#[derive(Debug)]
struct Stored {
    player_slot: PlayerSlot,
    to: Addressee,
    body: Range<usize>,
}

/// One input: the player's player slot, and the payload, a list of commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickInput<'a> {
    pub player_slot: PlayerSlot,
    pub payload: &'a [u8],
}

#[derive(Debug)]
struct Entry {
    player_slot: PlayerSlot,
    payload: Range<usize>,
}

impl TickInputs {
    pub fn push(&mut self, input: TickInput<'_>) {
        let start = self.payloads.len();
        self.payloads.extend_from_slice(input.payload);
        self.inputs.push(Entry {
            player_slot: input.player_slot,
            payload: start..self.payloads.len(),
        });
        let held = self.commands.len();
        let payload = &self.payloads[start..];
        let commands = &mut self.commands;
        let whole = Command::read(payload, |command, body| {
            commands.push(Stored {
                player_slot: input.player_slot,
                to: command.to,
                body: start + body.start..start + body.end,
            });
        });
        if !whole {
            self.commands.truncate(held);
            self.flawed.push(input.player_slot);
        }
    }

    /// The commands to `to` of the inputs, in the order of the inputs and of each input's
    /// commands.
    pub fn commands(&self, to: Addressee) -> impl Iterator<Item = PlayerCommand<'_>> {
        let ours = self.all().filter(move |&(of, _)| of == to);
        ours.map(|(_, command)| command)
    }

    /// Every command of the inputs, with what it goes to, in the order of the inputs and of each
    /// input's commands.
    pub fn all(&self) -> impl Iterator<Item = (Addressee, PlayerCommand<'_>)> {
        self.commands.iter().enumerate().map(|(place, held)| {
            let command = PlayerCommand {
                player_slot: held.player_slot,
                place,
                body: &self.payloads[held.body.clone()],
            };
            (held.to, command)
        })
    }

    /// The player of each input whose payload has a flaw, in order.
    pub fn flawed(&self) -> &[PlayerSlot] {
        &self.flawed
    }

    pub fn push_player_slot_event(&mut self, event: PlayerSlotEvent) {
        self.player_slot_events.push(event);
    }

    /// The player slot events in the order they were pushed.
    pub fn player_slot_events(&self) -> &[PlayerSlotEvent] {
        &self.player_slot_events
    }

    /// The inputs in the order they were pushed.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = TickInput<'_>> {
        self.inputs.iter().map(|entry| TickInput {
            player_slot: entry.player_slot,
            payload: &self.payloads[entry.payload.clone()],
        })
    }

    pub(crate) fn clear(&mut self) {
        self.inputs.clear();
        self.payloads.clear();
        self.commands.clear();
        self.flawed.clear();
        self.player_slot_events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Capability;
    use crate::player_slot_event::PlayerSlotEventKind;

    /// The player slot, the place and the body of each command to `to` that `inputs` holds.
    fn read(inputs: &TickInputs, to: Addressee) -> Vec<(u32, usize, &[u8])> {
        let commands = inputs.commands(to);
        commands
            .map(|command| (command.player_slot.get(), command.place, command.body))
            .collect()
    }

    #[test]
    fn each_input_gives_its_commands_once_and_a_flawed_one_none() {
        let orders = Addressee::Capability(Capability::Orders);
        let mode = Addressee::Capability(Capability::Mode);
        let to = |to, body| Command { to, body };
        // An empty body among the first's commands takes its place as the others do.
        let first = Command::encode(&[
            to(orders, b"ab"),
            to(mode, b"m"),
            to(Addressee::Players, b"s"),
            to(orders, b""),
            to(orders, b"c"),
        ]);
        // The second's last command is cut short: none of its commands count.
        let whole = Command::encode(&[to(orders, b"x"), to(orders, b"yz")]);
        let cut = &whole[..whole.len() - 1];
        let third = Command::encode(&[to(orders, b"d")]);
        let mut inputs = TickInputs::default();
        for (player_slot, payload) in [(1, first.as_slice()), (2, cut), (3, &third)] {
            let player_slot = PlayerSlot::new(player_slot);
            inputs.push(TickInput {
                player_slot,
                payload,
            });
        }
        // Each command's place counts every command of the tick before it, of any addressee:
        // the first's five at 0 to 4, the third's at 5, as the cut one holds none.
        assert_eq!(
            read(&inputs, orders),
            [(1, 0, &b"ab"[..]), (1, 3, b""), (1, 4, b"c"), (3, 5, b"d")]
        );
        assert_eq!(read(&inputs, mode), [(1, 1, &b"m"[..])]);
        assert_eq!(read(&inputs, Addressee::Players), [(1, 2, &b"s"[..])]);
        let all: Vec<(Addressee, usize)> = inputs
            .all()
            .map(|(to, command)| (to, command.place))
            .collect();
        assert_eq!(
            all,
            [
                (orders, 0),
                (mode, 1),
                (Addressee::Players, 2),
                (orders, 3),
                (orders, 4),
                (orders, 5)
            ]
        );
        assert_eq!(inputs.iter().len(), 3);
        assert_eq!(inputs.flawed(), [PlayerSlot::new(2)]);
        let joined = PlayerSlotEvent {
            player_slot: PlayerSlot::new(2),
            kind: PlayerSlotEventKind::Joined,
        };
        inputs.push_player_slot_event(joined);
        assert_eq!(inputs.player_slot_events(), [joined]);
        inputs.clear();
        assert_eq!(read(&inputs, orders), []);
        assert_eq!(inputs.flawed(), []);
        assert_eq!(inputs.player_slot_events(), []);
    }
}
