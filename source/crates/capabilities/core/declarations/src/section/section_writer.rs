use campfire_codec::{TomlTable, TomlValue};
use serde::Serialize;

use crate::section::Section;
use crate::section::patched::Patched;

/// A data table a tool writes from the engine's own types, as the import writes its files: each
/// section under its key, whole or as its patch, and the table's own keys from the patch of the
/// type that reads them, so a renamed key or field breaks the tool's build
/// (03-game-scripting.md, A section has a partial form).
#[derive(Debug, Default)]
pub struct SectionWriter {
    table: TomlTable,
}

impl SectionWriter {
    /// Writes `section` whole under its key.
    pub fn section<S: Section + Serialize>(&mut self, section: &S) -> &mut Self {
        self.put(S::KEY, TomlValue::of(section))
    }

    /// Writes `patch`, a part of an `S`, under `S`'s key.
    pub fn patch<S: Section + Patched>(&mut self, patch: &S::Patch) -> &mut Self {
        self.put(S::KEY, TomlValue::of(patch))
    }

    /// Writes `value`, which `S` reads, under `S`'s key: a section whose read a registry picks
    /// by kind, as `ActionsData`'s, which no one type writes.
    pub fn value<S: Section>(&mut self, value: TomlValue) -> &mut Self {
        self.put(S::KEY, value)
    }

    /// Writes `value` under `key`, a key the file that holds the table reads beside its
    /// sections, by the constant its reader names it by, as the mode's `ScriptOrder::KEY`.
    pub fn key<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> &mut Self {
        self.put(key, TomlValue::of(value))
    }

    /// Writes each key of `patch`, of the type that reads the table's own keys, at the table's
    /// top.
    pub fn own<T: Patched>(&mut self, patch: &T::Patch) -> &mut Self {
        let own: TomlTable = TomlValue::of(patch).read().expect("a patch writes a table");
        for (key, value) in own.into_entries() {
            self.put(&key, value);
        }
        self
    }

    /// The table it wrote.
    pub fn into_table(self) -> TomlTable {
        self.table
    }

    fn put(&mut self, key: &str, value: TomlValue) -> &mut Self {
        debug_assert!(!self.table.holds(key), "a table holds `{key}` once");
        self.table.insert(key.to_owned(), value);
        self
    }
}
