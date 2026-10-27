use super::*;

mod before {
    #![expect(
        clippy::disallowed_methods,
        reason = "a derive of JsonSchema calls serde_json::to_value"
    )]

    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    pub(super) struct Ask {
        pub(super) _first: u32,
    }
}

mod after {
    #![expect(
        clippy::disallowed_methods,
        reason = "a derive of JsonSchema calls serde_json::to_value"
    )]

    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    pub(super) struct Ask {
        pub(super) _first: u32,
        pub(super) _second: u32,
    }
}

#[test]
fn the_fingerprint_moves_with_a_field_of_either_side() {
    // Two types of one name, one field apart, alone and as either side of a pair.
    let before = Fingerprint::of::<before::Ask>();
    assert_eq!(before, Fingerprint::of::<before::Ask>());
    assert_ne!(before, Fingerprint::of::<after::Ask>());
    assert_ne!(
        Fingerprint::of::<(before::Ask, u8)>(),
        Fingerprint::of::<(after::Ask, u8)>()
    );
    assert_ne!(
        Fingerprint::of::<(u8, before::Ask)>(),
        Fingerprint::of::<(u8, after::Ask)>()
    );
    assert_eq!(format!("{}", Fingerprint(0xab)), "00000000000000ab");
}
