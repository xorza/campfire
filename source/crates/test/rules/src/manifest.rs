use std::collections::BTreeMap;

use campfire_codec::Toml;
use serde::Deserialize;

/// A crate's kind, which its manifest names under `[package.metadata.campfire]`
/// (14-workspace-alignment.md, Every crate declares its kind in its manifest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    /// One problem, with no engine under it.
    Problem,
    /// A problem crate that holds an IO: sockets, or the emulator's wires.
    Io,
    /// The one crate that makes a group of the banned calls.
    Proxy,
    /// What binds problem crates to Bevy's ECS, or a problem whose nouns are entities.
    Glue,
    /// What only tests build: fixtures, harnesses and the rules.
    Test,
    /// A binary that checks the engine and never ships.
    Check,
    /// A binary that ships.
    App,
}

impl Kind {
    /// Its name, as a manifest writes it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Problem => "problem",
            Self::Io => "io",
            Self::Proxy => "proxy",
            Self::Glue => "glue",
            Self::Test => "test",
            Self::Check => "check",
            Self::App => "app",
        }
    }
}

/// A crate's `Cargo.toml`, as the rules read it: every section a crate may hold, and no other, so
/// a manifest that holds one the rules do not know fails to read.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Manifest {
    #[serde(default)]
    package: Package,
    #[serde(default)]
    lib: Lib,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    dependencies: BTreeMap<String, Dependency>,
    #[serde(default)]
    dev_dependencies: BTreeMap<String, Dependency>,
    /// The dependencies of a platform, by its `cfg`.
    #[serde(default)]
    target: BTreeMap<String, TargetDependencies>,
    #[serde(default)]
    test: Vec<BuildTarget>,
    #[serde(default)]
    bench: Vec<BuildTarget>,
    #[serde(default)]
    lints: Lints,
}

/// The `[package]` section: its name and its metadata; the rest the workspace gives.
#[derive(Debug, Default, Deserialize)]
struct Package {
    #[serde(default)]
    name: String,
    #[serde(default)]
    metadata: Metadata,
}

#[derive(Debug, Default, Deserialize)]
struct Metadata {
    #[serde(default)]
    campfire: Campfire,
}

#[derive(Debug, Default, Deserialize)]
struct Campfire {
    kind: Option<Kind>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Lib {
    #[serde(default)]
    proc_macro: bool,
}

#[derive(Debug, Default, Deserialize)]
struct TargetDependencies {
    #[serde(default)]
    dependencies: BTreeMap<String, Dependency>,
}

#[derive(Debug, Default, Deserialize)]
struct Lints {
    #[serde(default)]
    workspace: bool,
}

/// A `[[test]]` or a `[[bench]]` target.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BuildTarget {
    #[serde(default)]
    required_features: Vec<String>,
}

/// A dependency, as a version alone or as a table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "DependencyForm")]
pub(crate) struct Dependency {
    /// Whether a feature turns it on.
    pub(crate) optional: bool,
    /// The features of its own it turns on.
    pub(crate) features: Vec<String>,
}

impl Dependency {
    /// Whether it turns its `internals` on.
    pub(crate) fn turns_internals_on(&self) -> bool {
        self.features.iter().any(|feature| feature == "internals")
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DependencyForm {
    Version(#[expect(dead_code, reason = "a version alone names no feature")] String),
    Table {
        #[serde(default)]
        optional: bool,
        #[serde(default)]
        features: Vec<String>,
    },
}

impl From<DependencyForm> for Dependency {
    fn from(form: DependencyForm) -> Self {
        match form {
            DependencyForm::Version(_) => Self::default(),
            DependencyForm::Table { optional, features } => Self { optional, features },
        }
    }
}

impl Manifest {
    /// The manifest `text` holds.
    pub(crate) fn parse(text: &str) -> Self {
        Toml::parse(text).unwrap_or_else(|error| panic!("a manifest reads: {error}"))
    }

    /// The package's name.
    pub(crate) fn name(&self) -> &str {
        &self.package.name
    }

    /// The kind the manifest names, if it names one.
    pub(crate) const fn kind(&self) -> Option<Kind> {
        self.package.metadata.campfire.kind
    }

    /// Each dependency, a target's among them, by its name, sorted.
    pub(crate) fn dependencies(&self) -> impl Iterator<Item = (&str, &Dependency)> {
        let targets = self.target.values().flat_map(|target| &target.dependencies);
        self.dependencies
            .iter()
            .chain(targets)
            .map(|(name, dependency)| (name.as_str(), dependency))
    }

    /// The dependencies the build requires, not those a feature turns on.
    pub(crate) fn required(&self) -> impl Iterator<Item = &str> {
        self.dependencies()
            .filter(|(_, dependency)| !dependency.optional)
            .map(|(name, _)| name)
    }

    /// Each dev-dependency, by its name, sorted.
    pub(crate) fn dev_dependencies(&self) -> impl Iterator<Item = (&str, &Dependency)> {
        self.dev_dependencies
            .iter()
            .map(|(name, dependency)| (name.as_str(), dependency))
    }

    /// The entries of the feature `name`, if the manifest has it.
    pub(crate) fn feature(&self, name: &str) -> Option<&[String]> {
        self.features.get(name).map(Vec::as_slice)
    }

    /// Whether its library is a procedural macro's.
    pub(crate) const fn is_proc_macro(&self) -> bool {
        self.lib.proc_macro
    }

    /// Whether it takes the workspace's lints.
    pub(crate) const fn takes_workspace_lints(&self) -> bool {
        self.lints.workspace
    }

    /// Whether a test or bench target of its own requires `internals`.
    pub(crate) fn requires_internals(&self) -> bool {
        self.test
            .iter()
            .chain(&self.bench)
            .any(|built| built.required_features.iter().any(|f| f == "internals"))
    }
}

/// The workspace's own `Cargo.toml`: the members its commands build when they name none.
#[derive(Debug, Deserialize)]
pub(crate) struct WorkspaceManifest {
    workspace: Workspace,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Workspace {
    #[serde(default)]
    default_members: Vec<String>,
}

impl WorkspaceManifest {
    /// The manifest `text` holds.
    pub(crate) fn parse(text: &str) -> Self {
        Toml::parse(text).unwrap_or_else(|error| panic!("the workspace's manifest reads: {error}"))
    }

    /// The paths of its default members, in its order.
    pub(crate) fn default_members(&self) -> &[String] {
        &self.workspace.default_members
    }
}

#[test]
fn a_manifest_reads_each_form_of_a_dependency_and_fails_on_a_section_it_does_not_know() {
    // A dependency by a version, a dotted key, a table, a dotted section and a target's, and a
    // dev-dependency; the target's condition is spelled in parts, so this file names no OS.
    let manifest = Manifest::parse(concat!(
        "[package]\nname = \"x\"\nversion.workspace = true\n\n",
        "[package.metadata.campfire]\nkind = \"glue\"\n\n",
        "[features]\ni = [\"dep:t\"]\n\n[dependencies]\n",
        "a = \"1\"\nb.workspace = true\nt = { workspace = true, optional = true }\n",
        "u = { workspace = true, features = [\"internals\"] }\n\n",
        "[dependencies.v]\nversion = \"2\"\n\n",
        "[target.'cfg(",
        "un",
        "ix)'.dependencies]\nl.workspace = true\n\n[dev-dependencies]\nd.workspace = true\n",
        "\n[[test]]\nname = \"s\"\nrequired-features = [\"internals\"]\n",
    ));
    let names: Vec<&str> = manifest.dependencies().map(|(name, _)| name).collect();
    assert_eq!(names, ["a", "b", "t", "u", "v", "l"]);
    let required: Vec<&str> = manifest.required().collect();
    assert_eq!(required, ["a", "b", "u", "v", "l"]);
    let dev: Vec<&str> = manifest.dev_dependencies().map(|(name, _)| name).collect();
    assert_eq!(dev, ["d"]);
    assert_eq!(manifest.name(), "x");
    assert_eq!(manifest.kind(), Some(Kind::Glue));
    assert_eq!(manifest.feature("i"), Some(&["dep:t".to_owned()][..]));
    assert!(manifest.requires_internals() && !manifest.is_proc_macro());
    assert!(!manifest.takes_workspace_lints());
    assert!(Manifest::parse("[lints]\nworkspace = true\n").takes_workspace_lints());
    let turns: Vec<&str> = manifest
        .dependencies()
        .filter(|(_, dependency)| dependency.turns_internals_on())
        .map(|(name, _)| name)
        .collect();
    assert_eq!(turns, ["u"]);
    let unknown = Toml::parse::<Manifest>("[build-dependencies]\na = \"1\"\n");
    assert!(unknown.is_err());
}
