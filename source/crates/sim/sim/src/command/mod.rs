use std::ops::Range;

use campfire_codec::{Binary, Decoded};
use serde::{Deserialize, Serialize, Serializer};

use crate::capability::Capability;

/// One command of a player input: what it goes to, and its body in that one's format. A payload
/// is a list of commands, so one input can carry commands for several capabilities and the
/// players, and each reads only its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command<'a> {
    pub to: Addressee,
    #[serde(serialize_with = "serialize_body")]
    pub body: &'a [u8],
}

/// What a command goes to: the players, whose select sets the units a player's orders command
/// (commanded-units.md, A select is the players' command), or a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Addressee {
    Players,
    Capability(Capability),
}

impl<'a> Command<'a> {
    /// The payload of `commands`, in order.
    pub fn encode(commands: &[Command<'_>]) -> Vec<u8> {
        let mut payload = Vec::new();
        Command::write(commands, &mut payload);
        payload
    }

    /// Appends the payload of `commands`, in order, to `out`.
    pub fn write(commands: &[Command<'_>], out: &mut Vec<u8>) {
        Binary::encode_into(commands, out);
    }

    /// Reads the commands of `payload` in one pass, giving each to `read` in order, with the
    /// place of its body in `payload`: whether the payload is exactly a list of commands. A
    /// client can send any bytes, so a reader of a payload with a flaw keeps none of the commands
    /// it was given.
    pub fn read(payload: &'a [u8], mut read: impl FnMut(Self, Range<usize>)) -> bool {
        let Ok(Decoded {
            value: count,
            mut rest,
        }) = Binary::take::<u64>(payload)
        else {
            return false;
        };
        for _ in 0..count {
            let Ok(Decoded {
                value: command,
                rest: after,
            }) = Binary::take::<Command<'_>>(rest)
            else {
                return false;
            };
            // A command's encoding ends with its body, so the body ends where what is left to
            // read begins.
            let end = payload.len() - after.len();
            read(command, end - command.body.len()..end);
            rest = after;
        }
        rest.is_empty()
    }
}

/// As postcard bytes, a length and the bytes; a plain slice would go through serde's element by
/// element sequence to reach the same encoding.
fn serialize_body<S: Serializer>(body: &&[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(body)
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::command::{Addressee, Command};

    impl Command<'_> {
        /// A payload of `bodies`, each a command to `to`, in order.
        pub fn payload<B: AsRef<[u8]>>(to: Addressee, bodies: &[B]) -> Vec<u8> {
            let commands: Vec<Command<'_>> = bodies
                .iter()
                .map(|body| Command {
                    to,
                    body: body.as_ref(),
                })
                .collect();
            Command::encode(&commands)
        }
    }
}

#[cfg(test)]
mod tests;
