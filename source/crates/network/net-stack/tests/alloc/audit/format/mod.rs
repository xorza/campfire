//! A failure's backtrace, cut to what names the allocation: the dependency frames that made the
//! allocating call, with one frame for each run through Bevy's scheduler, then the workspace's own
//! frames down to the fixture. Symbols lose their generic arguments.

use std::backtrace::Backtrace;
use std::env;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One frame of a printed backtrace: the symbol and the `at` location, when it has one.
#[derive(Debug)]
struct Frame<'a> {
    symbol: &'a str,
    at: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameKind {
    /// The standard library, which wraps every allocation.
    Std,
    /// The allocator and the audit, which wrap every window.
    Audit,
    /// A fixture, where a trace stops.
    Fixture,
    /// Another workspace file.
    Workspace,
    /// Bevy's scheduler and tracing's spans, which carry every system call.
    Scheduler,
    /// Another dependency.
    Dependency,
}

impl Frame<'_> {
    fn kind(&self) -> FrameKind {
        let kind = self.at.map_or(FrameKind::Dependency, classify);
        let symbol = self.symbol.trim_start_matches('<');
        let scheduler = ["bevy_ecs::", "bevy_app::", "in_scope<"]
            .iter()
            .any(|prefix| symbol.starts_with(prefix));
        if kind == FrameKind::Dependency && scheduler {
            FrameKind::Scheduler
        } else {
            kind
        }
    }
}

/// `CAMPFIRE_ALLOC_FULL_BT=1` prints the whole stack instead.
pub(super) fn frames(trace: &Backtrace) -> String {
    let printed = trace.to_string();
    if env::var_os("CAMPFIRE_ALLOC_FULL_BT").is_some() {
        return printed;
    }
    let mut out = String::new();
    let mut inside = false;
    let mut in_workspace = false;
    let mut last = FrameKind::Audit;
    for frame in parse(&printed) {
        let kind = frame.kind();
        // Everything before the allocator's first frame is the capture itself.
        inside |= kind == FrameKind::Audit;
        in_workspace |= kind == FrameKind::Workspace;
        let shown = inside
            && match kind {
                FrameKind::Std | FrameKind::Audit => false,
                FrameKind::Scheduler => !in_workspace && last != FrameKind::Scheduler,
                FrameKind::Dependency => !in_workspace,
                FrameKind::Workspace | FrameKind::Fixture => true,
            };
        if shown {
            writeln!(out, "  {}", short(frame.symbol)).ok();
            if let Some(at) = frame.at {
                writeln!(out, "      at {}", relative(at)).ok();
            }
            last = kind;
        }
        if kind == FrameKind::Fixture {
            break;
        }
    }
    if out.is_empty() { printed } else { out }
}

/// `symbol` without its generic arguments: `<A as B<C>>::f::<D>` reads `<A as B>::f`.
fn short(symbol: &str) -> String {
    let mut out = String::with_capacity(symbol.len());
    let mut depth = 0_usize;
    let mut previous = ' ';
    for char in symbol.chars() {
        let opens = out.ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == ':');
        match char {
            '<' if depth > 0 || opens => depth += 1,
            '>' if depth > 0 && previous != '-' => depth -= 1,
            _ if depth > 0 => {}
            _ => out.push(char),
        }
        previous = char;
    }
    out.replace("::::", "::").trim_end_matches("::").to_owned()
}

/// Reads std's printed form: a `N: symbol` line, then at most one `at path:line:col` line.
fn parse(printed: &str) -> impl Iterator<Item = Frame<'_>> {
    let mut lines = printed.lines().map(str::trim).peekable();
    core::iter::from_fn(move || {
        let symbol = loop {
            let line = lines.next()?;
            if let Some((index, symbol)) = line.split_once(": ")
                && index.bytes().all(|byte| byte.is_ascii_digit())
            {
                break symbol;
            }
        };
        let at = lines
            .next_if(|line| line.starts_with("at "))
            .map(|line| &line[3..]);
        Some(Frame { symbol, at })
    })
}

/// The workspace root, as many levels over this crate as the crate's path from the root has:
/// Cargo passes this file to rustc by its path from the root, which starts with the crate's.
fn workspace() -> &'static Path {
    let depth = Path::new(file!())
        .components()
        .take_while(|part| part.as_os_str() != "tests")
        .count();
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(depth)
        .expect("the workspace root is over the crate")
}

/// The audit binary's directory from the workspace root, as Cargo passes this file to rustc.
fn alloc_dir() -> &'static Path {
    Path::new(file!())
        .ancestors()
        .nth(3)
        .expect("this file is `alloc/audit/format/mod.rs`")
}

/// `at` as a path from the workspace root, when it is in the workspace. std prints a location
/// under the working directory, which is this crate's, as `./`.
fn workspace_path(at: &str) -> Option<PathBuf> {
    let path = at.split(':').next().unwrap_or(at);
    let path = match path.strip_prefix("./") {
        Some(local) => Path::new(env!("CARGO_MANIFEST_DIR")).join(local),
        None => PathBuf::from(path),
    };
    path.strip_prefix(workspace()).ok().map(Path::to_path_buf)
}

fn relative(at: &str) -> String {
    match workspace_path(at) {
        Some(path) => {
            let position = at.find(':').map_or("", |colon| &at[colon..]);
            format!("{}{position}", path.display())
        }
        None => at.to_owned(),
    }
}

fn classify(at: &str) -> FrameKind {
    let Some(path) = workspace_path(at) else {
        return if at.starts_with("/rustc/") || at.contains("/rustlib/src/rust/library/") {
            FrameKind::Std
        } else {
            FrameKind::Dependency
        };
    };
    let alloc = alloc_dir();
    if path.starts_with(alloc.join("fixtures")) {
        FrameKind::Fixture
    } else if path.starts_with(alloc) {
        FrameKind::Audit
    } else {
        FrameKind::Workspace
    }
}

mod tests;
