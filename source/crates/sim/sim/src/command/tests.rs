use super::*;

#[test]
fn a_payload_is_exactly_a_list_of_commands() {
    let to = Addressee::Capability;
    let commands = [
        Command {
            to: to(Capability::Orders),
            body: b"ab",
        },
        Command {
            to: to(Capability::Combat),
            body: b"",
        },
        Command {
            to: Addressee::Players,
            body: b"c",
        },
    ];
    let payload = Command::encode(&commands);
    assert_eq!(
        Command::payload(to(Capability::Orders), &[&b"x"[..], b"yz"]),
        [2, 1, 5, 1, b'x', 1, 5, 2, b'y', b'z']
    );
    // 3 commands; each its addressee, a capability as 1 and the capability's index, orders 5 and
    // combat 0, or the players as 0, then the body's length and bytes.
    assert_eq!(payload, [3, 1, 5, 2, b'a', b'b', 1, 0, 0, 0, 1, b'c']);
    let decode = |bytes| {
        let mut read = Vec::new();
        Command::read(bytes, |command, _| read.push(command)).then_some(read)
    };
    assert_eq!(decode(&payload), Some(commands.to_vec()));
    // Each body's place: `ab` after the count, the addressee and its length, at 4; the empty
    // body where the second command's three bytes end, at 9; `c` at 11.
    let mut places = Vec::new();
    assert!(Command::read(&payload, |_, body| places.push(body)));
    assert_eq!(places, [4..6, 9..9, 11..12]);

    // A flaw anywhere gives no command, not the ones before it: here no capability 13, one past
    // the last, and no addressee 2, one past the players and a capability.
    let mut unknown = payload.clone();
    unknown[2] = u8::try_from(Capability::ALL.len()).unwrap();
    let mut nobody = payload.clone();
    nobody[1] = 2;
    let flawed = [
        &payload[..payload.len() - 1],
        &[payload.as_slice(), &[0]].concat(),
        &[&[4], &payload[1..]].concat(),
        &unknown,
        &nobody,
        &[],
    ];
    for bytes in flawed {
        assert_eq!(decode(bytes), None, "{bytes:?}");
    }
    assert_eq!(decode(&[0]), Some(Vec::new()));
}
