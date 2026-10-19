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

fn kind_of(name: &str, file: &str) -> FrameKind {
    AllocTrace::kind(name, Some(file), AllocTrace::relative(file).as_deref())
}

#[test]
fn a_frame_is_sorted_by_where_its_file_lies_and_by_its_symbol() {
    let root = AllocTrace::workspace_root().to_string_lossy().into_owned();
    let source = "crates/network/net-udp/src/recv_batch.rs";
    // DWARF's absolute path, and PDB's relative one, led by `.` at times.
    for file in [
        native(&format!("{root}/{source}")),
        native(source),
        native(&format!("./{source}")),
    ] {
        assert_eq!(AllocTrace::relative(&file).as_deref(), Some(source));
        assert_eq!(kind_of("f", &file), FrameKind::Source, "{file}");
    }
    let registry = Path::new(&root)
        .parent()
        .unwrap()
        .join("registry/bytes-1.12.1/src/bytes_mut.rs");
    let registry = registry.to_string_lossy();
    for (name, file, kind) in [
        (
            "f",
            "crates/network/net-stack/tests/alloc/fixtures/idle.rs",
            FrameKind::Fixture,
        ),
        (
            "f",
            "crates/test/measure/src/counting_allocator.rs",
            FrameKind::Audit,
        ),
        (
            "__rust_alloc",
            "crates/network/net-stack/tests/alloc/main.rs",
            FrameKind::Audit,
        ),
        (
            "f",
            "crates/network/net-stack/tests/scenario.rs",
            FrameKind::Source,
        ),
        (
            "f",
            "/rustc/0123/library/core/src/ops/function.rs",
            FrameKind::Std,
        ),
        (
            "f",
            "/home/a/.rustup/toolchains/x/lib/rustlib/src/rust/library/core/src/ops.rs",
            FrameKind::Std,
        ),
        ("bytes::BytesMut::reserve", &registry, FrameKind::Dependency),
        (
            "bevy_ecs::system::System::run",
            &registry,
            FrameKind::Scheduler,
        ),
        ("<bevy_app::App>::update", &registry, FrameKind::Scheduler),
    ] {
        assert_eq!(kind_of(name, &native(file)), kind, "{name} in {file}");
    }
}

#[test]
fn a_trace_shows_the_allocating_call_and_the_workspaces_frames_down_to_the_fixture() {
    // The capture's own frame and the allocator's go; a dependency's frames before the
    // workspace's show, a run of the scheduler as one; the workspace's show through the fixture's
    // first, and the test's wrappers past it go.
    let dependency = "/home/a/.cargo/registry/src/x/src/lib.rs";
    let rendered = AllocTrace::kept_frames(
        [
            symbol("backtrace::capture", dependency, 1),
            symbol("track", "crates/test/measure/src/counting_allocator.rs", 2),
            symbol(
                "__rust_alloc",
                "crates/network/net-stack/tests/alloc/main.rs",
                15,
            ),
            symbol(
                "alloc::raw_vec::grow",
                "/rustc/0/library/alloc/src/raw_vec.rs",
                3,
            ),
            symbol("bevy_replicon::send<u8>", dependency, 4),
            symbol("bevy_ecs::system::run", dependency, 5),
            symbol("bevy_ecs::schedule::run", dependency, 6),
            symbol(
                "campfire_net_stepper::Stepper::frame_step",
                "crates/network/net-stepper/src/stepper/mod.rs",
                7,
            ),
            symbol("bevy_ecs::world::run", dependency, 8),
            symbol(
                "idle::fixture",
                "crates/network/net-stack/tests/alloc/fixtures/idle.rs",
                9,
            ),
            symbol(
                "idle::wrapper",
                "crates/network/net-stack/tests/alloc/fixtures/idle.rs",
                10,
            ),
        ]
        .into_iter(),
    );
    let expected = concat!(
        "   0: bevy_replicon::send\n            at /home/a/.cargo/registry/src/x/src/lib.rs:4:9\n",
        "   1: bevy_ecs::system::run\n            at /home/a/.cargo/registry/src/x/src/lib.rs:5:9\n",
        "   2: campfire_net_stepper::Stepper::frame_step\n",
        "            at crates/network/net-stepper/src/stepper/mod.rs:7:9\n",
        "   3: idle::fixture\n            at crates/network/net-stack/tests/alloc/fixtures/idle.rs:9:9\n",
    );
    assert_eq!(rendered, native_dependency(expected, dependency));
    assert_eq!(AllocTrace::kept_frames(iter::empty()), "");
}

/// `expected` with the dependency's path as the platform writes it.
fn native_dependency(expected: &str, dependency: &str) -> String {
    expected.replace(dependency, &native(dependency))
}

#[test]
fn a_name_drops_its_generic_arguments_and_keeps_the_trait_form() {
    for (name, short) in [
        ("alloc::vec::Vec<u8>::push", "alloc::vec::Vec::push"),
        ("<A as B<C>>::f::<D>", "<A as B>::f"),
        // `->` inside an argument list closes nothing.
        (
            "FunctionSystem<fn() -> Result<(), E>, S>::run",
            "FunctionSystem::run",
        ),
        ("pass<core::iter::FlatMap<a::B<c>, d>>", "pass"),
        (
            "<campfire_quic::ThreadFixture>::pass",
            "<campfire_quic::ThreadFixture>::pass",
        ),
        ("run<alloc::thread::f::{closure_env#1}>", "run"),
        ("{closure#0}", "{closure#0}"),
    ] {
        assert_eq!(AllocTrace::short(name), short, "{name}");
    }
}
