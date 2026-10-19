use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_common::Tick;

/// What the match's scripts printed with `print` and `debug` in a tick, which the server logs after
/// it, so the sim logs nothing. Only a debug build records a print: a release one drops each, so an
/// untrusted script cannot fill a server's log. Not state: nothing in the sim reads it.
#[derive(Resource, Debug, Default)]
pub struct ScriptPrints {
    tick: Tick,
    /// Every print's text, one run each.
    text: String,
    entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
struct Entry {
    kind: PrintKind,
    text: Range<usize>,
}

/// How a script printed: with `print`, or with `debug`, which gives the line and the column of
/// its call when Rhai knows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintKind {
    Print,
    Debug {
        line: Option<u32>,
        column: Option<u32>,
    },
}

/// A print as `ScriptPrints` gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptPrint<'a> {
    pub kind: PrintKind,
    pub text: &'a str,
}

impl ScriptPrints {
    /// Records that a script printed `text` as `kind`.
    pub(crate) fn push(&mut self, kind: PrintKind, text: &str) {
        let start = self.text.len();
        self.text.push_str(text);
        self.entries.push(Entry {
            kind,
            text: start..self.text.len(),
        });
    }

    /// Takes the prints of `from` as those of `tick`, and empties `from`, keeping both buffers.
    pub(crate) fn take(&mut self, tick: Tick, from: &mut Self) {
        std::mem::swap(self, from);
        from.text.clear();
        from.entries.clear();
        self.tick = tick;
    }

    /// The tick whose prints the record holds.
    pub const fn tick(&self) -> Tick {
        self.tick
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The prints, in the order the scripts printed them.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = ScriptPrint<'_>> {
        self.entries.iter().map(|entry| ScriptPrint {
            kind: entry.kind,
            text: &self.text[entry.text.clone()],
        })
    }
}
