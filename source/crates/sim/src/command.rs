use serde::{Deserialize, Serialize, Serializer};

/// One command of a player input: the name of the capability it goes to, and its body in that
/// capability's format. A payload is a list of commands, so one input can carry commands for
/// several capabilities, and each capability reads only its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command<'a> {
    pub capability: &'a str,
    #[serde(serialize_with = "serialize_body")]
    pub body: &'a [u8],
}

impl<'a> Command<'a> {
    /// The payload of `commands`, in order.
    pub fn encode(commands: &[Command<'_>]) -> Vec<u8> {
        postcard::to_allocvec(commands).expect("commands always encode")
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
    pub fn bodies(payload: &'a [u8], capability: &'a str) -> impl Iterator<Item = &'a [u8]> {
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
                capability: "orders",
                body: b"ab",
            },
            Command {
                capability: "character",
                body: b"",
            },
            Command {
                capability: "orders",
                body: b"c",
            },
        ];
        let payload = Command::encode(&commands);
        // 3 commands; each the name's length and bytes, then the body's length and bytes.
        assert_eq!(
            payload,
            [
                &[3, 6][..],
                b"orders",
                &[2],
                b"ab",
                &[9],
                b"character",
                &[0, 6],
                b"orders",
                &[1],
                b"c",
            ]
            .concat()
        );
        let decoded: Vec<_> = Command::decode(&payload).unwrap().collect();
        assert_eq!(decoded, commands);
        let orders: Vec<_> = Command::bodies(&payload, "orders").collect();
        assert_eq!(orders, [&b"ab"[..], b"c"]);
        assert_eq!(Command::bodies(&payload, "abilities").count(), 0);

        // A flaw anywhere gives no command, not the ones before it.
        let mut invalid_name = payload.clone();
        invalid_name[2] = 0xFF;
        let flawed = [
            &payload[..payload.len() - 1],
            &[payload.as_slice(), &[0]].concat(),
            &[&[4], &payload[1..]].concat(),
            &invalid_name,
            &[],
        ];
        for bytes in flawed {
            assert!(Command::decode(bytes).is_none(), "{bytes:?}");
            assert_eq!(Command::bodies(bytes, "orders").count(), 0, "{bytes:?}");
        }
        assert_eq!(Command::decode(&[0]).unwrap().count(), 0);
    }
}
