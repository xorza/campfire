use crate::audit::format::{FrameKind, classify, parse, relative, short, workspace};

#[test]
fn short_drops_generic_arguments_and_keeps_the_trait_form() {
    assert_eq!(short("alloc::vec::Vec<u8>::push"), "alloc::vec::Vec::push");
    assert_eq!(short("<A as B<C>>::f::<D>"), "<A as B>::f");
    // `->` inside an argument list closes nothing.
    assert_eq!(
        short("FunctionSystem<fn() -> Result<(), E>, S>::run"),
        "FunctionSystem::run"
    );
    assert_eq!(short("frame_step"), "frame_step");
}

#[test]
fn parse_pairs_each_symbol_with_its_location() {
    let printed = "   0: track\n             at ./tests/alloc/allocator/mod.rs:43:21\n   1: \
                   bevy_replicon::client::receive_replication\n   2: frame_step\n             at \
                   ./src/client_server_stepper/mod.rs:522:47\n";
    let frames: Vec<_> = parse(printed)
        .map(|frame| (frame.symbol, frame.at))
        .collect();
    assert_eq!(
        frames,
        [
            ("track", Some("./tests/alloc/allocator/mod.rs:43:21")),
            ("bevy_replicon::client::receive_replication", None),
            (
                "frame_step",
                Some("./src/client_server_stepper/mod.rs:522:47")
            ),
        ]
    );
}

#[test]
fn classify_sorts_locations_by_owner() {
    let transport = format!("{}/crates/transport/src/lib.rs:1:1", workspace().display());
    let cases = [
        ("./tests/alloc/fixtures/idle.rs:8:5", FrameKind::Fixture),
        ("./tests/alloc/allocator/mod.rs:43:21", FrameKind::Audit),
        ("./src/stepper.rs:1:1", FrameKind::Workspace),
        (
            "/rustc/0123/library/core/src/ops/function.rs:250:5",
            FrameKind::Std,
        ),
        (
            "/home/a/.rustup/toolchains/x/lib/rustlib/src/rust/library/core/src/ops.rs:1:1",
            FrameKind::Std,
        ),
        (
            "/home/a/.cargo/registry/src/bytes-1.12.1/src/bytes.rs:677:18",
            FrameKind::Dependency,
        ),
    ];
    for (at, kind) in cases {
        assert_eq!(classify(at), kind, "{at}");
    }
    assert_eq!(classify(&transport), FrameKind::Workspace);
    assert_eq!(relative(&transport), "crates/transport/src/lib.rs:1:1");
    assert_eq!(
        relative("./tests/alloc/fixtures/idle.rs:8:5"),
        "crates/network/net-stack/tests/alloc/fixtures/idle.rs:8:5"
    );
}
