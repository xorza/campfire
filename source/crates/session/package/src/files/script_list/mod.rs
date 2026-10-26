use campfire_script::ListPlace;
use campfire_values::{PackageName, PackagePath, ScriptSource};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The scripts a mode runs, as its `data/mode.toml` lists them under `scripts`, in the order
/// their hooks answer an event, and which of them is the mode's own: the one entry marked
/// `mode = true`, or the list's only one (03-game-scripting.md, Rules scripts in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptOrder {
    /// Each once, at most `ListPlace::LIMIT`.
    scripts: Vec<ScriptSource>,
    mode: ListPlace,
}

impl ScriptOrder {
    /// The key the mode's file lists its scripts under.
    pub const KEY: &'static str = "scripts";

    /// The list of `source` alone, which is the mode's own.
    pub fn one(source: ScriptSource) -> Self {
        Self {
            scripts: vec![source],
            mode: ListPlace::FIRST,
        }
    }

    /// The scripts, in the list's order.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &ScriptSource> {
        self.scripts.iter()
    }

    /// The place of the mode's own script.
    pub(crate) const fn mode(&self) -> ListPlace {
        self.mode
    }
}

/// An entry of the list: a path alone, or a table of an optional package, a path and an
/// optional mark.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Written {
    Own(PackagePath),
    Table(Entry),
}

/// An entry of the list in its table's form, which reads and writes the same keys.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<PackageName>,
    path: PackagePath,
    #[serde(default, skip_serializing_if = "is_unmarked")]
    mode: bool,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's test of a field to skip takes it by reference"
)]
const fn is_unmarked(mode: &bool) -> bool {
    !*mode
}

/// Each entry in its table's form, the mode's own marked in a list of more than one, which
/// `Deserialize` reads back to it.
impl Serialize for ScriptOrder {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let marks = self.scripts.len() > 1;
        serializer.collect_seq(self.scripts.iter().enumerate().map(|(at, source)| Entry {
            package: source.package().cloned(),
            path: source.path().clone(),
            mode: marks && at == self.mode.index(),
        }))
    }
}

/// A list of entries, each a path alone or a table of an optional package, a path and an
/// optional mark: one or more, each once, at most one marked, and one marked in a list of more
/// than one.
impl<'de> Deserialize<'de> for ScriptOrder {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let written = Vec::<Written>::deserialize(deserializer)?;
        if written.len() > ListPlace::LIMIT {
            return Err(D::Error::custom(format_args!(
                "a mode lists at most {} scripts",
                ListPlace::LIMIT
            )));
        }
        let mut scripts = Vec::with_capacity(written.len());
        let mut marked = Vec::new();
        for (at, entry) in written.into_iter().enumerate() {
            let (source, mode) = match entry {
                Written::Own(path) => (ScriptSource::Own(path), false),
                Written::Table(Entry {
                    package,
                    path,
                    mode,
                }) => (ScriptSource::of(package, path), mode),
            };
            if scripts.contains(&source) {
                return Err(D::Error::custom(format_args!(
                    "lists the script {} twice",
                    source.path().as_str()
                )));
            }
            if mode {
                marked.push(at);
            }
            scripts.push(source);
        }
        let mode = match (marked.as_slice(), scripts.len()) {
            (_, 0) => return Err(D::Error::custom("lists no script")),
            (&[at], _) => at,
            ([], 1) => 0,
            ([], _) => {
                return Err(D::Error::custom(
                    "marks none of its scripts the mode's own, `mode = true`",
                ));
            }
            _ => {
                return Err(D::Error::custom(
                    "marks more than one script the mode's own",
                ));
            }
        };
        let mode = ListPlace::new(mode).expect("the list is within its limit");
        Ok(Self { scripts, mode })
    }
}

#[cfg(test)]
mod tests;
