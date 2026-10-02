use std::mem;

use serde::{Deserialize, Serialize, Serializer};

use crate::capability::Capability;

/// One command of a player input: the capability it goes to, and its body in that capability's
/// format. A payload is a list of commands, so one input can carry commands for
/// several capabilities, and each capability reads only its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command<'a> {
    pub capability: Capability,
    #[serde(serialize_with = "serialize_body")]
    pub body: &'a [u8],
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
        *out = postcard::to_extend(commands, mem::take(out)).expect("commands always encode");
    }

    /// A payload of `bodies`, each a command to `capability`, in order.
    pub fn payload<B: AsRef<[u8]>>(capability: Capability, bodies: &[B]) -> Vec<u8> {
        let commands: Vec<Command<'_>> = bodies
            .iter()
            .map(|body| Command {
                capability,
                body: body.as_ref(),
            })
            .collect();
        Command::encode(&commands)
    }

    /// Reads the commands of `payload` in one pass, giving each to `read` in order: whether the
    /// payload is exactly a list of commands. A client can send any bytes, so a reader of a
    /// payload with a flaw keeps none of the commands it was given.
    pub fn read(payload: &'a [u8], mut read: impl FnMut(Command<'a>)) -> bool {
        let Ok((count, mut rest)) = postcard::take_from_bytes::<u64>(payload) else {
            return false;
        };
        for _ in 0..count {
            let Ok((command, after)) = postcard::take_from_bytes::<Command<'_>>(rest) else {
                return false;
            };
            read(command);
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

#[cfg(test)]
mod tests;
