use campfire_script::ListPlace;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::files::package_name::PackageName;

/// The scripts a mode runs, as its `data/mode.toml` lists them under `scripts`, in the order
/// their hooks answer an event, and which of them is the mode's own: the one entry marked
/// `mode = true`, or the list's only one (03-game-scripting.md, Rules scripts in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptOrder {
    /// Each once, at most `ListPlace::LIMIT`.
    scripts: Vec<ScriptSource>,
    mode: ListPlace,
}

/// A script of a mode's list: one of its package's, by its path, or one of a rules package it
/// depends on, `{ package, path }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptSource {
    Own(PackagePath),
    Rules {
        package: PackageName,
        path: PackagePath,
    },
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

impl ScriptSource {
    /// The script at `path` of `package`, the mode's own with none.
    fn of(package: Option<PackageName>, path: PackagePath) -> Self {
        match package {
            Some(package) => Self::Rules { package, path },
            None => Self::Own(path),
        }
    }

    /// The script's path in its package.
    pub(crate) const fn path(&self) -> &PackagePath {
        match self {
            Self::Own(path) | Self::Rules { path, .. } => path,
        }
    }

    /// The rules package that holds it; none for the mode's own.
    pub(crate) const fn package(&self) -> Option<&PackageName> {
        match self {
            Self::Own(_) => None,
            Self::Rules { package, .. } => Some(package),
        }
    }
}

/// A script by itself, as a mode's bot names its: a path alone, or `{ package, path }`, which
/// `Deserialize` reads back to it.
impl Serialize for ScriptSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Own(path) => path.serialize(serializer),
            Self::Rules { package, path } => Source {
                package: Some(package.clone()),
                path: path.clone(),
            }
            .serialize(serializer),
        }
    }
}

/// A script by itself, as a mode's bot names its: a path alone, or a table of an optional
/// package and a path.
impl<'de> Deserialize<'de> for ScriptSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match WrittenSource::deserialize(deserializer)? {
            WrittenSource::Own(path) => Self::Own(path),
            WrittenSource::Table(Source { package, path }) => Self::of(package, path),
        })
    }
}

/// A script by itself as a file writes it: a path alone, or a table.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WrittenSource {
    Own(PackagePath),
    Table(Source),
}

/// A script by itself in its table's form, which reads and writes the same keys.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<PackageName>,
    path: PackagePath,
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
mod tests {
    use std::fmt::Write;

    use campfire_codec::Toml;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct File {
        scripts: ScriptOrder,
    }

    fn read(text: &str) -> Result<ScriptOrder, String> {
        Toml::parse::<File>(text)
            .map(|file| file.scripts)
            .map_err(|error| error.to_string())
    }

    fn own(path: &str) -> ScriptSource {
        ScriptSource::Own(PackagePath::parse(path).unwrap())
    }

    #[test]
    fn a_list_names_its_scripts_in_order_and_marks_the_mode_s_own() {
        // The only entry is the mode's own, marked or not.
        let one = read("scripts = [\"scripts/mode.rhai\"]").unwrap();
        assert_eq!(
            one.iter().cloned().collect::<Vec<_>>(),
            [own("scripts/mode.rhai")]
        );
        assert_eq!(one.mode(), ListPlace::FIRST);
        let marked = read("scripts = [{ path = \"scripts/mode.rhai\", mode = true }]").unwrap();
        assert_eq!(marked, one);
        // In a list of more, the marked one, here the second: a rules package's script.
        let list = read(
            "scripts = [\"scripts/first.rhai\", { package = \"test-rules\", path = \
             \"scripts/mode.rhai\", mode = true }, { path = \"scripts/last.rhai\" }]",
        )
        .unwrap();
        let rules = ScriptSource::Rules {
            package: PackageName::new("test-rules").unwrap(),
            path: PackagePath::parse("scripts/mode.rhai").unwrap(),
        };
        let held: Vec<_> = list.iter().cloned().collect();
        assert_eq!(
            held,
            [own("scripts/first.rhai"), rules, own("scripts/last.rhai")]
        );
        assert_eq!(list.mode(), ListPlace::new(1).unwrap());
        // Each writes its entries as tables, the mode's marked in a list of more than one, and
        // reads back to itself; a list of one is `one`'s.
        let written = |order: &ScriptOrder| {
            let text = Toml::write(&std::collections::BTreeMap::from([(
                ScriptOrder::KEY,
                order,
            )]));
            read(&text).unwrap()
        };
        assert_eq!(written(&list), list);
        assert_eq!(written(&one), one);
        assert_eq!(ScriptOrder::one(own("scripts/mode.rhai")), one);
        // The same path of two packages is two scripts.
        let two = "scripts = [{ path = \"scripts/mode.rhai\", mode = true }, { package = \
                   \"test-rules\", path = \"scripts/mode.rhai\" }]";
        assert_eq!(read(two).unwrap().iter().len(), 2);
    }

    #[test]
    fn a_list_of_no_script_one_twice_or_no_one_mark_fails() {
        let refused = |text: &str, why: &str| {
            let error = read(text).unwrap_err();
            assert!(error.contains(why), "{text}: {error}");
        };
        refused("scripts = []", "lists no script");
        refused(
            "scripts = [\"scripts/a.rhai\", { path = \"scripts/a.rhai\", mode = true }]",
            "lists the script scripts/a.rhai twice",
        );
        refused(
            "scripts = [\"scripts/a.rhai\", \"scripts/b.rhai\"]",
            "marks none of its scripts",
        );
        refused(
            "scripts = [{ path = \"scripts/a.rhai\", mode = true }, { path = \"scripts/b.rhai\", \
             mode = true }]",
            "marks more than one",
        );
        let mut many = String::from("scripts = [{ path = \"scripts/0.rhai\", mode = true }");
        for at in 1..=ListPlace::LIMIT {
            write!(many, ", \"scripts/{at}.rhai\"").unwrap();
        }
        many.push(']');
        refused(&many, "at most 256 scripts");
        // An entry with a key it does not take.
        assert!(read("scripts = [{ path = \"scripts/a.rhai\", ends = true }]").is_err());
    }
}
