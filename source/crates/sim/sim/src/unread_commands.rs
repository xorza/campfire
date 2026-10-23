use std::slice;

use bevy_ecs::resource::Resource;
use campfire_common::PlayerSlot;

use crate::command::Addressee;
use crate::tick_inputs::TickInputs;

/// The commands of the last tick that nothing read: a payload that is no list of commands, a body
/// its addressee cannot decode, and a command to an addressee no module of the match reads. A
/// client may send any bytes, so a server plays on, and the harness fails a test on any it does not
/// take, so a sender whose encoder its decoder disagrees with shows (02-engine-core.md, A command
/// nothing reads is counted). Not state: the tick's start empties it.
#[derive(Resource, Debug, Default)]
pub struct UnreadCommands {
    entries: Vec<UnreadCommand>,
}

/// A command of a player's that nothing read, by why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnreadCommand {
    pub player_slot: PlayerSlot,
    pub cause: UnreadCause,
}

/// Why nothing read a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadCause {
    /// Its payload is no list of commands, so none of its commands reaches an addressee.
    Flawed,
    /// Its addressee cannot decode its body.
    Undecodable(Addressee),
    /// No module of the match reads its addressee.
    Unaddressed(Addressee),
}

impl UnreadCommands {
    /// Starts the tick of `inputs` afresh: each payload with a flaw unread.
    pub fn begin(&mut self, inputs: &TickInputs) {
        self.entries.clear();
        let flawed = inputs.flawed().iter().map(|&player_slot| UnreadCommand {
            player_slot,
            cause: UnreadCause::Flawed,
        });
        self.entries.extend(flawed);
    }

    /// Keeps a command of `player_slot`'s that nothing read, for `cause`.
    pub fn push(&mut self, player_slot: PlayerSlot, cause: UnreadCause) {
        self.entries.push(UnreadCommand { player_slot, cause });
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> slice::Iter<'_, UnreadCommand> {
        self.entries.iter()
    }
}

impl<'a> IntoIterator for &'a UnreadCommands {
    type Item = &'a UnreadCommand;
    type IntoIter = slice::Iter<'a, UnreadCommand>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::mem;

    use bevy_ecs::world::World;

    use crate::unread_commands::{UnreadCommand, UnreadCommands};

    impl UnreadCommands {
        /// Panics on a command of the last tick of `world` that nothing read: a test that sends
        /// one takes it.
        pub fn assert_read(world: &World) {
            let Some(unread) = world.get_resource::<Self>() else {
                return;
            };
            assert!(
                unread.is_empty(),
                "commands nothing read: {:?}; a test that sends them takes them",
                unread.entries
            );
        }

        /// Takes the commands of the last tick of `world` that nothing read.
        pub fn take(world: &mut World) -> Vec<UnreadCommand> {
            world
                .get_resource_mut::<Self>()
                .map_or_else(Vec::new, |mut unread| mem::take(&mut unread.entries))
        }
    }
}
