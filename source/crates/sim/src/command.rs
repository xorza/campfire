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

    /// The commands of `payload`, in order; `None` unless the payload is exactly a list of
    /// commands. A client can send any bytes, so a payload with a flaw gives no command at all.
    pub fn decode(payload: &'a [u8]) -> Option<impl Iterator<Item = Command<'a>> + Clone> {
        let (count, list) = postcard::take_from_bytes::<u64>(payload).ok()?;
        let mut rest = list;
        for _ in 0..count {
            (_, rest) = postcard::take_from_bytes::<Command<'_>>(rest).ok()?;
        }
        if !rest.is_empty() {
            return None;
        }
        let mut rest = list;
        Some((0..count).map(move |_| {
            let (command, after) =
                postcard::take_from_bytes(rest).expect("every command decoded once already");
            rest = after;
            command
        }))
    }

    /// The bodies of the commands in `payload` that go to `capability`, in order.
    pub fn bodies(payload: &'a [u8], capability: Capability) -> impl Iterator<Item = &'a [u8]> {
        Command::decode(payload)
            .into_iter()
            .flatten()
            .filter(move |command| command.capability == capability)
            .map(|command| command.body)
    }
}

/// As postcard bytes, a length and the bytes; a plain slice would go through serde's element by
/// element sequence to reach the same encoding.
fn serialize_body<S: Serializer>(body: &&[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_is_exactly_a_list_of_commands() {
        let commands = [
            Command {
                capability: Capability::Orders,
                body: b"ab",
            },
            Command {
                capability: Capability::Character,
                body: b"",
            },
            Command {
                capability: Capability::Orders,
                body: b"c",
            },
        ];
        let payload = Command::encode(&commands);
        assert_eq!(
            Command::payload(Capability::Orders, &[&b"x"[..], b"yz"]),
            [2, 5, 1, b'x', 5, 2, b'y', b'z']
        );
        // 3 commands; each the capability's index, orders 5 and character 6, then the body's
        // length and bytes.
        assert_eq!(payload, [3, 5, 2, b'a', b'b', 6, 0, 5, 1, b'c']);
        let decoded: Vec<_> = Command::decode(&payload).unwrap().collect();
        assert_eq!(decoded, commands);
        let orders: Vec<_> = Command::bodies(&payload, Capability::Orders).collect();
        assert_eq!(orders, [&b"ab"[..], b"c"]);
        assert_eq!(Command::bodies(&payload, Capability::Abilities).count(), 0);

        // A flaw anywhere gives no command, not the ones before it: here no capability 12.
        let mut unknown = payload.clone();
        unknown[6] = 12;
        let flawed = [
            &payload[..payload.len() - 1],
            &[payload.as_slice(), &[0]].concat(),
            &[&[4], &payload[1..]].concat(),
            &unknown,
            &[],
        ];
        for bytes in flawed {
            assert!(Command::decode(bytes).is_none(), "{bytes:?}");
            assert_eq!(
                Command::bodies(bytes, Capability::Orders).count(),
                0,
                "{bytes:?}"
            );
        }
        assert_eq!(Command::decode(&[0]).unwrap().count(), 0);
    }
}
