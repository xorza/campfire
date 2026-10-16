use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::toml::Toml;

/// A TOML value as a test reads and edits it whole, codec's own: a document of the engine holds
/// no dates. An integer reads before a float, so each keeps its kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TomlNode {
    Integer(i64),
    Float(f64),
    Text(String),
    Flag(bool),
    List(Vec<Self>),
    Table(BTreeMap<String, Self>),
}

/// A TOML document a test edits by the path of a key, its keys and array indices joined by dots,
/// so an edit holds whatever value the document has there, and writes back: the keys of each
/// table come back sorted. A bad document or path is the test's mistake, and panics.
#[derive(Debug, Clone, PartialEq)]
pub struct TomlDocument(TomlNode);

/// What a path does at a table that lacks its next key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Missing {
    /// Adds the key, an empty table.
    Add,
    Fail,
}

impl TomlDocument {
    pub fn parse(text: &str) -> Self {
        Self(TomlNode::Table(
            Toml::parse(text).expect("a test's TOML parses"),
        ))
    }

    /// The document's top table, to walk and edit whole.
    pub fn table_mut(&mut self) -> &mut BTreeMap<String, TomlNode> {
        match &mut self.0 {
            TomlNode::Table(table) => table,
            _ => unreachable!("a document is a table at its top"),
        }
    }

    /// Sets the value at `path` to `value`, as TOML writes it, the tables on its way added when
    /// missing.
    pub fn set(&mut self, path: &str, value: &str) {
        let mut parsed: BTreeMap<String, TomlNode> =
            Toml::parse(&format!("value = {value}")).expect("a test's TOML value parses");
        let value = parsed.remove("value").expect("the value just parsed");
        self.at_path(path, Missing::Add, |parent, key| {
            parent.put(key, value);
        });
    }

    /// Removes the value at `path`.
    pub fn remove(&mut self, path: &str) {
        self.at_path(path, Missing::Fail, |parent, key| {
            drop(parent.take(key));
        });
    }

    /// Gives the key at `path` the name `name`, in the same table.
    pub fn rename(&mut self, path: &str, name: &str) {
        self.at_path(path, Missing::Fail, |parent, key| {
            let value = parent.take(key);
            parent.put(name, value);
        });
    }

    /// The document as TOML text.
    pub fn text(&self) -> String {
        Toml::write(&self.0)
    }

    /// Makes `edit` to the value that holds the last key of `path`.
    fn at_path(&mut self, path: &str, missing: Missing, edit: impl FnOnce(&mut TomlNode, &str)) {
        let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
        let parent =
            parent
                .split('.')
                .filter(|key| !key.is_empty())
                .fold(&mut self.0, |value, key| match value {
                    TomlNode::Table(table) => match missing {
                        Missing::Add => table
                            .entry(key.to_owned())
                            .or_insert_with(|| TomlNode::Table(BTreeMap::new())),
                        Missing::Fail => table
                            .get_mut(key)
                            .unwrap_or_else(|| panic!("no {key} on the way to {path}")),
                    },
                    TomlNode::List(list) => &mut list[list_index(key)],
                    _ => panic!("{path} passes through a value"),
                });
        edit(parent, key);
    }
}

impl TomlNode {
    /// Puts `value` at `key` of it, a table's key or a list's index.
    fn put(&mut self, key: &str, value: Self) {
        match self {
            Self::Table(table) => drop(table.insert(key.to_owned(), value)),
            Self::List(list) => list[list_index(key)] = value,
            _ => panic!("{key} is inside a value"),
        }
    }

    /// Takes the value at `key` of it, a table's key or a list's index, out of it.
    fn take(&mut self, key: &str) -> Self {
        match self {
            Self::Table(table) => table
                .remove(key)
                .unwrap_or_else(|| panic!("no {key} to remove")),
            Self::List(list) => list.remove(list_index(key)),
            _ => panic!("{key} is inside a value"),
        }
    }
}

/// The index `key` names in a list.
fn list_index(key: &str) -> usize {
    key.parse()
        .unwrap_or_else(|_| panic!("{key} indexes an array"))
}
