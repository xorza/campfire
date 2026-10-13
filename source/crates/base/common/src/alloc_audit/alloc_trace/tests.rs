use std::iter;
use std::path::MAIN_SEPARATOR_STR;

use super::*;

/// `path` with the separator of the running platform's debug info, so one table covers
/// DWARF's and PDB's shapes.
fn native(path: &str) -> String {
    path.replace('/', MAIN_SEPARATOR_STR)
}

fn symbol(name: &str, file: &str, line: u32) -> TraceSymbol {
    TraceSymbol {
        name: Some(name.to_owned()),
        file: Some(native(file)),
        line,
        column: 9,
    }
}

fn kind_of(file: &str) -> Option<FrameKind> {
    AllocTrace::relative(file).map(|relative| AllocTrace::kind(&relative))
}

#[test]
fn a_frame_is_kept_by_where_its_file_lies_in_the_workspace() {
    let root = AllocTrace::workspace_root().to_string_lossy().into_owned();
    let source = "crates/network/quic/src/recv_batch.rs";
    // DWARF's absolute path, and PDB's relative one, led by `.` at times.
    for file in [
        native(&format!("{root}/{source}")),
        native(source),
        native(&format!("./{source}")),
    ] {
        assert_eq!(AllocTrace::relative(&file).as_deref(), Some(source));
        assert_eq!(kind_of(&file), Some(FrameKind::Source), "{file}");
    }
    for (file, kind) in [
        (
            "crates/network/quic/tests/alloc/main.rs",
            FrameKind::Fixture,
        ),
        (
            "crates/base/common/src/alloc_audit/mod.rs",
            FrameKind::Other,
        ),
        ("crates/base/common/src/codec/sink.rs", FrameKind::Source),
        ("crates/network/quic/tests/rejoin.rs", FrameKind::Other),
        ("docs/design/15-quic-transport.md", FrameKind::Other),
    ] {
        assert_eq!(kind_of(&native(file)), Some(kind), "{file}");
    }
    // A dependency's path is absolute outside the workspace. Std's is absolute on Unix and
    // has no root on Windows, where its first part, empty, is no `crates`.
    let registry = Path::new(&root)
        .parent()
        .unwrap()
        .join("registry/bytes-1.12.1/src/bytes_mut.rs");
    assert_eq!(kind_of(&registry.to_string_lossy()), None);
    assert!(matches!(
        kind_of("/rustc/0000000000000000000000000000000000000000/library/std/src/rt.rs"),
        None | Some(FrameKind::Other)
    ));
}

#[test]
fn a_trace_shows_the_workspaces_frames_from_the_allocation_to_the_fixtures_call() {
    let rendered = AllocTrace::workspace_frames(
        [
            symbol(
                "__rust_alloc",
                "crates/network/quic/tests/alloc/main.rs",
                15,
            ),
            symbol(
                "alloc::raw_vec::grow",
                "/rustc/0/library/alloc/src/raw_vec.rs",
                3,
            ),
            symbol(
                "campfire_quic::outbox::Outbox::push",
                "crates/network/quic/src/outbox.rs",
                34,
            ),
            symbol(
                "campfire_common::alloc_audit::AllocAudit::run",
                "crates/base/common/src/alloc_audit/mod.rs",
                100,
            ),
            symbol(
                "alloc::fixture",
                "crates/network/quic/tests/alloc/main.rs",
                40,
            ),
            symbol(
                "alloc::wrapper",
                "crates/network/quic/tests/alloc/main.rs",
                60,
            ),
        ]
        .into_iter(),
    );
    assert_eq!(
        rendered,
        "   0: campfire_quic::outbox::Outbox::push\n            at crates/network/quic/src/outbox.rs:34:9\n   1: alloc::fixture\n            at crates/network/quic/tests/alloc/main.rs:40:9\n"
    );
    assert_eq!(AllocTrace::workspace_frames(iter::empty()), "");
    for (name, short) in [
        ("pass<core::iter::FlatMap<a::B<c>, d>>", "pass"),
        (
            "<campfire_quic::ThreadFixture>::pass",
            "<campfire_quic::ThreadFixture>::pass",
        ),
        ("<a::B<c> as d::E>", "<a::B<c> as d::E>"),
        ("run<alloc::thread::f::{closure_env#1}>", "run"),
        ("{closure#0}", "{closure#0}"),
    ] {
        assert_eq!(AllocTrace::without_generics(name), short);
    }
}
