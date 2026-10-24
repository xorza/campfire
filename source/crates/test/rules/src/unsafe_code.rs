use crate::allowlist::Allowlist;
use crate::named_path::NamedPath;
use crate::workspace::Workspace;

/// The files that hold `unsafe`, each for an outside API that asks for it (02-engine-core.md,
/// Unsafe only where an outside API asks).
const UNSAFE_FILES: [&str; 9] = [
    "crates/base/platform/src/unix.rs",
    "crates/base/platform/src/windows.rs",
    "crates/capabilities/core/declarations/src/spawn_parts.rs",
    "crates/network/net-port-map/src/listening.rs",
    "crates/network/net-socket/src/udp_port.rs",
    "crates/network/net-sync/src/synced_local_timeline.rs",
    "crates/sim/math/src/wide_division/x86_64.rs",
    "crates/sim/sim/src/state_registry/decoded_state.rs",
    "crates/test/measure/src/counting_allocator.rs",
];

/// Whether the code of `text` writes `unsafe`, a comment's or a string's mention aside.
fn writes_unsafe(text: &str) -> bool {
    text.contains("unsafe") && NamedPath::of(text).iter().any(|path| path.text == "unsafe")
}

pub(crate) fn only_the_listed_files_write_unsafe(workspace: &Workspace) {
    let listed = Allowlist::new(&UNSAFE_FILES);
    let unlisted: Vec<&str> = (workspace.sources().iter())
        .filter(|file| writes_unsafe(&file.text))
        .filter(|file| !listed.allows(|path| *path == file.path))
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(unlisted, Vec::<&str>::new());
    assert_eq!(listed.unused(), Vec::<&&str>::new());
    // A block, a function and an impl write it; a comment and a string do not.
    for (code, writes) in [
        ("fn f() { unsafe { g() } }", true),
        ("unsafe fn f() {}", true),
        ("unsafe impl Send for A {}", true),
        ("// unsafe { g() }\nconst K: &str = \"unsafe\";", false),
    ] {
        assert_eq!(writes_unsafe(code), writes, "{code}");
    }
}
