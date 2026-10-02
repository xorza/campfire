#[cfg(unix)]
use std::os::unix::fs::symlink as link_file;
#[cfg(windows)]
use std::os::windows::fs::symlink_file as link_file;

use campfire_content::Fingerprint;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::*;
use crate::package_store::PackageStore;

fn write(dir: &Path, path: &str, text: &str) {
    let path = dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn a_package_reads_once_and_its_fingerprint_hashes_the_sorted_file_list() {
    // Under a directory that goes when the test ends, passed or failed.
    let scratch = TempDir::new().unwrap();
    let root = scratch.path().to_owned();
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
    let read = dir.read().unwrap();
    assert_eq!(read.fingerprint(), expected);
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
    let memory = other.join(Path::new("../one")).read().unwrap();
    assert_eq!(memory.fingerprint(), expected);
    let listed: Vec<_> = memory.files_under("data").collect();
    assert_eq!(listed, [&PackagePath::parse("data/a.toml").unwrap()]);
    let manifest = PackagePath::parse("manifest.toml").unwrap();
    assert_eq!(memory.read_text(&manifest).unwrap(), "m");
    let missing = PackagePath::parse("scripts/x.rhai").unwrap();
    assert!(matches!(
        memory.read_text(&missing),
        Err(ContentError::Io { .. })
    ));
    // What was read stays as it was read: a later change on disk reaches neither its text nor
    // its fingerprint.
    write(&package, "data/b/c.toml", "");
    let files: Vec<_> = dir
        .read()
        .unwrap()
        .files_under("data")
        .map(PackagePath::to_string)
        .collect();
    assert_eq!(files, ["data/a.toml", "data/b/c.toml"]);
    assert_eq!(read.files_under("data").count(), 1);
    assert_eq!(read.files_under("scripts").count(), 0);
    fs::remove_dir_all(package.join("data/b")).unwrap();

    // Any change to a file's bytes, or its path, gives another fingerprint.
    write(&package, "data/a.toml", "ac");
    let changed = dir.read().unwrap().fingerprint();
    assert_ne!(changed, expected);
    let data = PackagePath::parse("data/a.toml").unwrap();
    assert_eq!(read.read_text(&data).unwrap(), "ab");
    fs::rename(package.join("data"), package.join("date")).unwrap();
    assert_ne!(dir.read().unwrap().fingerprint(), changed);

    // A store finds each package under a root by fingerprint, and searches no deeper than a
    // manifest. A package that does not read is the store's failure, and costs no other.
    write(&root, "nested/two/manifest.toml", "n");
    write(&root, "one/inner/manifest.toml", "i");
    write(&root, "bad/manifest.toml", "b");
    link_file(root.join("bad/manifest.toml"), root.join("bad/link")).unwrap();
    let store = PackageStore::scan(&root).unwrap();
    let one = dir.read().unwrap().fingerprint();
    let two = PackageDir::new(root.join("nested/two"))
        .read()
        .unwrap()
        .fingerprint();
    for fingerprint in [one, two] {
        assert_eq!(
            store.get(fingerprint).map(PackageFiles::fingerprint),
            Some(fingerprint)
        );
    }
    let inner = PackageDir::new(root.join("one/inner"))
        .read()
        .unwrap()
        .fingerprint();
    assert!(store.get(inner).is_none());
    let failures = store.failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].dir, root.join("bad"));
    assert!(
        matches!(&failures[0].error, ContentError::NotAFile(path) if *path == root.join("bad/link"))
    );

    // A link is no file of a package, and a name no package path spells is no file of one.
    link_file(package.join("manifest.toml"), package.join("link")).unwrap();
    assert!(
        matches!(dir.read(), Err(ContentError::NotAFile(path)) if path == package.join("link"))
    );
    fs::remove_file(package.join("link")).unwrap();
    #[cfg(unix)]
    {
        write(&package, "a\\b.toml", "");
        assert!(
            matches!(dir.read(), Err(ContentError::NotPath(path)) if path == package.join("a\\b.toml"))
        );
    }
}

fn walker() -> PackageDir {
    PackageDir::new(PackageDir::workspace("test/heroes/walker"))
}

#[test]
fn a_package_reads_its_own_files_only() {
    #[derive(Debug, serde::Deserialize)]
    struct Hero {
        name: String,
        slots: BTreeMap<String, Vec<String>>,
        actions: BTreeMap<String, toml::Table>,
    }
    #[derive(Debug, serde::Deserialize)]
    struct Named {
        #[expect(dead_code, reason = "the read fails before any use")]
        script: PackagePath,
    }
    let path = |text| PackagePath::parse(text).unwrap();
    let walker = walker().read().unwrap();
    let hero: Hero = walker.read_data(&path("data/avatar.toml")).unwrap();
    assert_eq!(hero.name, "hero-name");
    assert_eq!(hero.slots["basic"][2], "third");
    assert!(hero.actions.contains_key("first"));
    let script = walker.read_text(&path("scripts/strike.rhai")).unwrap();
    assert!(script.starts_with("fn on_resolve(ctx, caster, target)"));

    for text in [
        "../walker/data/avatar.toml",
        "/etc/hosts",
        "data/../../x",
        "",
    ] {
        assert_eq!(PackagePath::parse(text), None, "{text}");
    }
    // Data that names a path outside the package does not read.
    assert!(toml::from_str::<Named>(r#"script = "../x.rhai""#).is_err());
    assert!(matches!(
        walker.read_text(&path("scripts/missing.rhai")),
        Err(ContentError::Io { .. })
    ));
    assert!(matches!(
        walker.read_data::<Hero>(&path("scripts/strike.rhai")),
        Err(ContentError::Data { .. })
    ));
}
