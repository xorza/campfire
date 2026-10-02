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
    let decode = |bytes| {
        let mut read = Vec::new();
        Command::read(bytes, |command| read.push(command)).then_some(read)
    };
    assert_eq!(decode(&payload), Some(commands.to_vec()));

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
        assert_eq!(decode(bytes), None, "{bytes:?}");
    }
    assert_eq!(decode(&[0]), Some(Vec::new()));
}
