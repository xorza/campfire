use super::*;

#[test]
fn a_name_lowercases_and_replaces_what_a_declared_name_lacks() {
    for (original, name) in [
        (&b"AmericaTankCrusader"[..], "americatankcrusader"),
        (b"tournament desert", "tournament_desert"),
        (b"usa07-taskforces", "usa07_taskforces"),
        (b"*Waypoints/Waypoint", "x_waypoints_waypoint"),
        (b"_art review", "x_art_review"),
        (b"3v3", "x3v3"),
        (b"Grav\xFEl", "grav_l"),
        (b"", "x"),
    ] {
        let imported = ImportName::of(original);
        assert_eq!(imported.as_str(), name);
        assert_eq!(imported.declared().unwrap().as_str(), name);
        assert_eq!(imported.map().unwrap().as_str(), name);
    }
}

/// A name of 64 bytes is a declared name, one of 65 none, and an id or a suffix as long as the
/// two parts and a `_` make it.
#[test]
fn a_name_past_the_bytes_a_name_holds_is_refused() {
    let long = ImportName::of(&[b'a'; DeclaredName::MAX_LEN]);
    assert_eq!(
        long.declared().unwrap().as_str().len(),
        DeclaredName::MAX_LEN
    );
    let past = "a".repeat(DeclaredName::MAX_LEN + 1);
    let refused = Err(NameError::TooLong(past.clone()));
    assert_eq!(ImportName::of(past.as_bytes()).declared(), refused);
    assert_eq!(
        ImportName::of(past.as_bytes()).map().map(drop),
        refused.map(drop)
    );
    let short = ImportName::of(&[b'a'; DeclaredName::MAX_LEN - 1]);
    assert_eq!(
        short.id("x"),
        Err(NameError::TooLong(format!("x_{}", short.as_str())))
    );
    assert!(short.suffixed("a").is_err() && short.suffixed("").is_ok());
    assert!(
        ImportName::of(&[b'a'; DeclaredName::MAX_LEN - 2])
            .id("b")
            .is_ok()
    );
}
