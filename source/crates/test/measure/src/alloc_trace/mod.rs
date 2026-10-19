use std::env;
use std::fmt::Write as _;
use std::path::{self, Path};

use backtrace::{Backtrace, BacktraceFrame};

/// The backtrace of one counted allocation, captured with no symbols, which only a failure
/// resolves: a passing window allocates nothing, so it walks no stack.
#[derive(Debug)]
pub(crate) struct AllocTrace(Backtrace);

/// What a frame of a trace is, by where its file lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    /// The standard library, which wraps every allocation.
    Std,
    /// This crate and the allocator's shims, which wrap every window.
    Audit,
    /// A crate's `tests/alloc` suite: the fixture's call, where a trace ends.
    Fixture,
    /// A crate's own code.
    Source,
    /// Bevy's scheduler and tracing's spans, which carry every system's call.
    Scheduler,
    /// Another dependency, which may make the allocating call.
    Dependency,
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

/// The starts of the symbols of Bevy's scheduler and of tracing's spans.
const SCHEDULER: [&str; 3] = ["bevy_ecs::", "bevy_app::", "in_scope<"];

impl AllocTrace {
    pub(crate) const fn new(backtrace: Backtrace) -> Self {
        Self(backtrace)
    }

    /// The call stack from the allocation to the fixture's call: the dependency frames that made
    /// the allocating call, one frame for each run through Bevy's scheduler, then the workspace's
    /// own frames; the whole backtrace when `CAMPFIRE_ALLOC_FULL_BT` is set, or when the filter
    /// keeps no frame.
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
        let mut out = Self::kept_frames(symbols);
        if out.is_empty() {
            write!(
                out,
                "(the filter keeps no frame; the whole backtrace:)\n{:?}",
                self.0
            )
            .expect("a String takes every write");
        }
        out
    }

    /// The frames of a backtrace a failure shows, innermost first: those past the allocator's,
    /// the standard library's and the audit's left out, a dependency's only before the first of
    /// the workspace's, and one of a run of the scheduler's, through the fixture's first.
    fn kept_frames(symbols: impl Iterator<Item = TraceSymbol>) -> String {
        let mut out = String::new();
        let mut index = 0;
        let mut past_allocator = false;
        let mut in_workspace = false;
        let mut last = FrameKind::Audit;
        for symbol in symbols {
            let name = symbol.name.as_deref().unwrap_or("<unknown>");
            let relative = symbol.file.as_deref().and_then(Self::relative);
            let kind = Self::kind(name, symbol.file.as_deref(), relative.as_deref());
            // Each frame before the allocator's is the capture's own.
            past_allocator |= kind == FrameKind::Audit;
            in_workspace |= matches!(kind, FrameKind::Source | FrameKind::Fixture);
            let shown = past_allocator
                && match kind {
                    FrameKind::Std | FrameKind::Audit => false,
                    FrameKind::Scheduler => !in_workspace && last != FrameKind::Scheduler,
                    FrameKind::Dependency => !in_workspace,
                    FrameKind::Source | FrameKind::Fixture => true,
                };
            if shown {
                let file = relative
                    .as_deref()
                    .or(symbol.file.as_deref())
                    .unwrap_or("<unknown>");
                writeln!(out, "  {index:>2}: {}", Self::short(name))
                    .expect("a String takes every write");
                writeln!(
                    out,
                    "            at {file}:{}:{}",
                    symbol.line, symbol.column
                )
                .expect("a String takes every write");
                index += 1;
                last = kind;
            }
            if kind == FrameKind::Fixture {
                break;
            }
        }
        out
    }

    /// `name` without its generic arguments, which name each closure and iterator type the call
    /// took and say nothing the file and line do not: `<A as B<C>>::f::<D>` reads `<A as B>::f`.
    fn short(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        let mut depth = 0_usize;
        let mut previous = ' ';
        for character in name.chars() {
            let opens = out.ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == ':');
            match character {
                '<' if depth > 0 || opens => depth += 1,
                // `->` inside an argument list closes nothing.
                '>' if depth > 0 && previous != '-' => depth -= 1,
                _ if depth > 0 => {}
                _ => out.push(character),
            }
            previous = character;
        }
        out.replace("::::", "::").trim_end_matches("::").to_owned()
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

    /// The workspace's root, three levels over this crate's manifest: `crates/test/measure`.
    fn workspace_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("measure's manifest lies in a group of the workspace's crates directory")
    }

    /// What the frame of the symbol `name`, in `file`, is: a workspace file by its crate, its
    /// `src/` or `tests/`, and this crate's; any other by its path and its symbol.
    fn kind(name: &str, file: Option<&str>, relative: Option<&str>) -> FrameKind {
        if name.starts_with(ALLOCATOR_SHIM) {
            return FrameKind::Audit;
        }
        if let Some(path) = relative.and_then(|relative| relative.strip_prefix("crates/")) {
            let inside = ["/src/", "/tests/"]
                .into_iter()
                .filter_map(|own| path.find(own))
                .min();
            if let Some(at) = inside {
                return match (&path[..at], &path[at + 1..]) {
                    ("test/measure", inside) if inside.starts_with("src/") => FrameKind::Audit,
                    (_, inside) if inside.starts_with("tests/alloc/") => FrameKind::Fixture,
                    _ => FrameKind::Source,
                };
            }
        }
        let path = file.unwrap_or_default().replace('\\', "/");
        let std = path.contains("/rustc/")
            || path.starts_with("rustc/")
            || path.contains("/rustlib/src/rust/library/");
        let symbol = name.trim_start_matches('<');
        if std {
            FrameKind::Std
        } else if SCHEDULER.iter().any(|start| symbol.starts_with(start)) {
            FrameKind::Scheduler
        } else {
            FrameKind::Dependency
        }
    }
}

#[cfg(test)]
mod tests;
