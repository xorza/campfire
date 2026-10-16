use std::env;
use std::fmt::Write as _;
use std::path::{self, Path};

use backtrace::{Backtrace, BacktraceFrame};

/// The backtrace of one counted allocation, captured with no symbols, which only a failure
/// resolves: a passing window allocates nothing, so it walks no stack.
#[derive(Debug)]
pub(crate) struct AllocTrace(Backtrace);

/// Where a frame's file lies in the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    /// A crate's own code, where the allocation usually is.
    Source,
    /// A crate's `tests/alloc` suite: the fixture's call, once for each trace.
    Fixture,
    /// The audit itself, a dependency or std.
    Other,
}

/// What the filter reads of one symbol of a resolved frame.
#[derive(Debug)]
struct TraceSymbol {
    name: Option<String>,
    file: Option<String>,
    line: u32,
    column: u32,
}

/// With it set, a failure prints each backtrace whole.
const FULL_BACKTRACE: &str = "CAMPFIRE_ALLOC_FULL_BT";

/// The start of the names of the shims `#[global_allocator]` makes: `__rust_alloc`,
/// `__rust_realloc` and their kin.
const ALLOCATOR_SHIM: &str = "__rust_";

impl AllocTrace {
    pub(crate) const fn new(backtrace: Backtrace) -> Self {
        Self(backtrace)
    }

    /// The call stack from the fixture's call down to the allocation, its frames the workspace's
    /// own; the whole backtrace when `CAMPFIRE_ALLOC_FULL_BT` is set, or when no frame is the
    /// workspace's.
    pub(crate) fn render(&mut self) -> String {
        self.0.resolve();
        if env::var_os(FULL_BACKTRACE).is_some() {
            return format!("{:?}", self.0);
        }
        let symbols = self
            .0
            .frames()
            .iter()
            .flat_map(BacktraceFrame::symbols)
            .map(|symbol| TraceSymbol {
                name: symbol.name().map(|name| format!("{name:#}")),
                file: symbol
                    .filename()
                    .map(|file| file.to_string_lossy().into_owned()),
                line: symbol.lineno().unwrap_or(0),
                column: symbol.colno().unwrap_or(0),
            });
        let mut out = Self::workspace_frames(symbols);
        if out.is_empty() {
            write!(
                out,
                "(no frame is the workspace's; the whole backtrace:)\n{:?}",
                self.0
            )
            .expect("a String takes every write");
        }
        out
    }

    /// The workspace's own symbols of a backtrace, innermost first, through the fixture's first.
    fn workspace_frames(symbols: impl Iterator<Item = TraceSymbol>) -> String {
        let mut out = String::new();
        let mut index = 0;
        let mut fixture_seen = false;
        for symbol in symbols {
            // `#[global_allocator]` places the allocator's shims in the file that installs it, a
            // crate's `tests/alloc`, and they are no fixture's call.
            if symbol
                .name
                .as_deref()
                .is_some_and(|name| name.starts_with(ALLOCATOR_SHIM))
            {
                continue;
            }
            let Some(relative) = symbol.file.as_deref().and_then(Self::relative) else {
                continue;
            };
            match Self::kind(&relative) {
                FrameKind::Other => continue,
                // The frames past the fixture's first are the test's wrappers.
                FrameKind::Fixture if fixture_seen => break,
                FrameKind::Fixture => fixture_seen = true,
                FrameKind::Source => {}
            }
            let name = symbol
                .name
                .as_deref()
                .map_or("<unknown>", Self::without_generics);
            writeln!(out, "  {index:>2}: {name}").expect("a String takes every write");
            writeln!(
                out,
                "            at {relative}:{}:{}",
                symbol.line, symbol.column
            )
            .expect("a String takes every write");
            index += 1;
        }
        out
    }

    /// `name` without the generic arguments that end it, which name each closure and iterator
    /// type the call took and say nothing the file and line do not.
    fn without_generics(name: &str) -> &str {
        if !name.ends_with('>') {
            return name;
        }
        let mut depth = 0_usize;
        for (index, character) in name.char_indices().rev() {
            match character {
                '>' => depth += 1,
                '<' => {
                    depth -= 1;
                    if depth == 0 {
                        // A name that starts with `<` is a qualified path, `<T as Trait>::f`.
                        return if index == 0 { name } else { &name[..index] };
                    }
                }
                _ => {}
            }
        }
        name
    }

    /// A frame's file relative to the workspace, `/`-separated, or `None` when it lies outside.
    /// Debug info joins a workspace file onto its build's directory on Linux and macOS, so the
    /// workspace's absolute root goes; a PDB on Windows keeps the path cargo passed, which is
    /// relative for a workspace member and absolute for a dependency or std.
    fn relative(file: &str) -> Option<String> {
        let root = Self::workspace_root();
        let relative = match file.strip_prefix(root.to_string_lossy().as_ref()) {
            Some(tail) => tail.trim_start_matches(path::is_separator),
            None if Path::new(file).is_absolute() => return None,
            None => file,
        };
        let relative = relative.replace(path::MAIN_SEPARATOR, "/");
        Some(relative.trim_start_matches("./").to_owned())
    }

    /// The workspace's root, three levels over this crate's manifest: `crates/base/common`.
    fn workspace_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("common's manifest lies in a group of the workspace's crates directory")
    }

    /// A crate's folder lies under `crates/`, at the depth of its group, and its code under its
    /// `src/` or `tests/`.
    fn kind(relative: &str) -> FrameKind {
        let Some(path) = relative.strip_prefix("crates/") else {
            return FrameKind::Other;
        };
        let inside = ["/src/", "/tests/"]
            .into_iter()
            .filter_map(|own| path.find(own))
            .min();
        let Some(at) = inside else {
            return FrameKind::Other;
        };
        match (&path[..at], &path[at + 1..]) {
            ("base/common", inside) if inside.starts_with("src/alloc_audit/") => FrameKind::Other,
            (_, inside) if inside.starts_with("tests/alloc/") => FrameKind::Fixture,
            (_, inside) if inside.starts_with("src/") => FrameKind::Source,
            _ => FrameKind::Other,
        }
    }
}

#[cfg(test)]
mod tests;
