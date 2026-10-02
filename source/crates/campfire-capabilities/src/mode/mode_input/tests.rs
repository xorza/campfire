use super::*;

#[test]
fn an_input_is_its_name_then_its_value_in_its_declared_type() {
    let types = |name: &str| match name {
        "hero" => Some(InputType::String),
        "spells" => Some(InputType::StringList),
        _ => None,
    };
    let hero = ModeInput {
        name: "hero",
        value: InputValue::String("rime"),
    };
    let spells = ModeInput {
        name: "spells",
        value: InputValue::StringList(vec!["haste", "mend"]),
    };
    // A string is its length and bytes; a list its count, then each string.
    let body = hero.encode();
    assert_eq!(body, [&[4][..], b"hero", &[4], b"rime"].concat());
    assert_eq!(ModeInput::decode(&body, types), Some(hero.clone()));
    let body = spells.encode();
    let expected = [&[6][..], b"spells", &[2, 5], b"haste", &[4], b"mend"].concat();
    assert_eq!(body, expected);
    assert_eq!(ModeInput::decode(&body, types), Some(spells.clone()));

    // A name the mode does not declare, a value of another type, a short body and trailing
    // bytes: none decodes.
    let unknown = ModeInput {
        name: "pet",
        value: InputValue::String("owl"),
    };
    let as_list = ModeInput {
        name: "hero",
        value: InputValue::StringList(vec!["rime"]),
    };
    let hero = hero.encode();
    for flawed in [
        unknown.encode(),
        as_list.encode(),
        hero[..hero.len() - 1].to_vec(),
        [hero.as_slice(), &[0]].concat(),
    ] {
        assert_eq!(ModeInput::decode(&flawed, types), None, "{flawed:?}");
    }

    // A payload holds each as a command of the mode, the capability of index 12.
    let payload = ModeInput::payload(&[spells]);
    assert_eq!(payload[..2], [1, 12]);
    let mut bodies = Vec::new();
    assert!(Command::read(&payload, |command, _| {
        assert_eq!(command.capability, Capability::Mode);
        bodies.push(command.body);
    }));
    assert_eq!(bodies, [expected.as_slice()]);
}
