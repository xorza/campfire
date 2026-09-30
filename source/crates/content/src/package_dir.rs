use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;

use crate::error::ContentError;
use crate::package_path::PackagePath;

/// A package's files on disk, as the workspace holds them before a package is built.
#[derive(Debug, Clone)]
pub struct PackageDir {
    root: PathBuf,
}

impl PackageDir {
    pub fn new(root: impl Into<PathBuf>) -> PackageDir {
        PackageDir { root: root.into() }
    }

    /// The TOML data file at `path`, read as a `T`.
    pub fn read_data<T: DeserializeOwned>(&self, path: &PackagePath) -> Result<T, ContentError> {
        toml::from_str(&self.read_text(path)?).map_err(|error| ContentError::Data {
            path: path.clone(),
            error,
        })
    }

    /// The text of the file at `path`, such as a script's source.
    pub fn read_text(&self, path: &PackagePath) -> Result<String, ContentError> {
        fs::read_to_string(self.root.join(path.as_path())).map_err(|error| ContentError::Io {
            path: path.clone(),
            error,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn husk() -> PackageDir {
        PackageDir::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/moba/heroes/husk"
        ))
    }

    #[test]
    fn a_package_reads_its_own_files_only() {
        #[derive(Debug, serde::Deserialize)]
        struct Hero {
            name: String,
            slots: Vec<String>,
            abilities: BTreeMap<String, toml::Table>,
        }
        #[derive(Debug, serde::Deserialize)]
        struct Named {
            #[expect(dead_code, reason = "the read fails before any use")]
            script: PackagePath,
        }
        let path = |text| PackagePath::parse(text).unwrap();
        let hero: Hero = husk().read_data(&path("data/hero.toml")).unwrap();
        assert_eq!(hero.name, "Husk");
        assert_eq!(hero.slots[2], "lash_out");
        assert!(hero.abilities.contains_key("lash_out"));
        let script = husk().read_text(&path("scripts/lash_out.rhai")).unwrap();
        assert!(script.starts_with("fn on_cast(ctx, caster, target)"));

        for text in ["../husk/data/hero.toml", "/etc/hosts", "data/../../x", ""] {
            assert!(
                matches!(
                    PackagePath::parse(text),
                    Err(ContentError::OutsidePackage(_))
                ),
                "{text}"
            );
        }
        // Data that names a path outside the package does not read.
        assert!(toml::from_str::<Named>(r#"script = "../x.rhai""#).is_err());
        assert!(matches!(
            husk().read_text(&path("scripts/missing.rhai")),
            Err(ContentError::Io { .. })
        ));
        assert!(matches!(
            husk().read_data::<Hero>(&path("scripts/lash_out.rhai")),
            Err(ContentError::Data { .. })
        ));
    }
}
