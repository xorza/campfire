#[cfg(unix)]
use std::os::unix::fs::symlink as link_file;
#[cfg(windows)]
use std::os::windows::fs::symlink_file as link_file;
use std::{env, process};

use super::*;
use crate::package_store::PackageStore;

/// A new directory under the system's temporary one, named for the test.
fn scratch(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("campfire-package-{}-{name}", process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, path: &str, text: &str) {
    let path = dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn a_fingerprint_hashes_the_sorted_file_list() {
    let root = scratch("fingerprint");
    let package = root.join("one");
    write(&package, "manifest.toml", "m");
    write(&package, "data/a.toml", "ab");
    // Rows sorted by path bytes: "data/a.toml" before "manifest.toml". Postcard writes the
    // count 2, then each row as its path's length and bytes, its size as a varint, and the 32
    // bytes of its SHA-256.
    let list = [
        &[2, 11][..],
        b"data/a.toml",
        &[2],
        &Sha256::digest(b"ab"),
        &[13],
        b"manifest.toml",
        &[1],
        &Sha256::digest(b"m"),
    ]
    .concat();
    let expected = Fingerprint::new(Sha256::digest(&list).into());
    let dir = PackageDir::new(&package);
    assert_eq!(dir.fingerprint().unwrap(), expected);
    // The same files in memory, in a tree whose other files are outside the package, reached
    // through a path that leaves a sibling first, read and fingerprint the same.
    let tree: BTreeMap<PathBuf, Vec<u8>> = [
        ("pack/one/manifest.toml", "m"),
        ("pack/one/data/a.toml", "ab"),
        ("pack/other/manifest.toml", "o"),
    ]
    .into_iter()
    .map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec()))
    .collect();
    let other = PackageDir::in_memory(Arc::new(tree), "pack/other");
    let memory = other.join(Path::new("../one"));
    assert_eq!(memory.fingerprint().unwrap(), expected);
    let listed: Vec<_> = memory.files_under("data").unwrap();
    assert_eq!(listed, [PackagePath::parse("data/a.toml").unwrap()]);
    let manifest = PackagePath::parse("manifest.toml").unwrap();
    assert_eq!(memory.read_text(&manifest).unwrap(), "m");
    let missing = PackagePath::parse("scripts/x.rhai").unwrap();
    assert!(matches!(
        memory.read_text(&missing),
        Err(ContentError::Io { .. })
    ));
    write(&package, "data/b/c.toml", "");
    let files = dir.files_under("data").unwrap();
    let files: Vec<_> = files.iter().map(PackagePath::to_string).collect();
    assert_eq!(files, ["data/a.toml", "data/b/c.toml"]);
    assert!(dir.files_under("scripts").unwrap().is_empty());
    fs::remove_dir_all(package.join("data/b")).unwrap();

    // Any change to a file's bytes, or its path, gives another fingerprint.
    write(&package, "data/a.toml", "ac");
    let changed = dir.fingerprint().unwrap();
    assert_ne!(changed, expected);
    fs::rename(package.join("data"), package.join("date")).unwrap();
    assert_ne!(dir.fingerprint().unwrap(), changed);

    // A store finds each package under a root by fingerprint, and searches no deeper than a
    // manifest.
    write(&root, "nested/two/manifest.toml", "n");
    write(&root, "one/inner/manifest.toml", "i");
    let store = PackageStore::scan(&root).unwrap();
    let one = dir.fingerprint().unwrap();
    let two = PackageDir::new(root.join("nested/two"))
        .fingerprint()
        .unwrap();
    assert_eq!(store.get(one).unwrap().root(), package);
    assert_eq!(store.get(two).unwrap().root(), root.join("nested/two"));
    let inner = PackageDir::new(root.join("one/inner"))
        .fingerprint()
        .unwrap();
    assert!(store.get(inner).is_none());

    // A link is no file of a package.
    link_file(package.join("manifest.toml"), package.join("link")).unwrap();
    assert!(
        matches!(dir.fingerprint(), Err(ContentError::NotAFile(path)) if path == package.join("link"))
    );
    fs::remove_dir_all(root).unwrap();
}

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
    let hero: Hero = husk().read_data(&path("data/avatar.toml")).unwrap();
    assert_eq!(hero.name, "Husk");
    assert_eq!(hero.slots[2], "lash_out");
    assert!(hero.abilities.contains_key("lash_out"));
    let script = husk().read_text(&path("scripts/lash_out.rhai")).unwrap();
    assert!(script.starts_with("fn on_resolve(ctx, caster, target)"));

    for text in ["../husk/data/avatar.toml", "/etc/hosts", "data/../../x", ""] {
        assert_eq!(PackagePath::parse(text), None, "{text}");
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
