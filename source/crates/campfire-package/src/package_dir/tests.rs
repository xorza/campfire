use std::env;

use campfire_common::{Fingerprint, Toml};
use campfire_store::{DurableFile, Scratch};

use super::*;
use crate::package_store::PackageStore;

/// Writes the index of the package at `dir`, under `scratch`, from its files.
fn pack(scratch: &Scratch, dir: &str) {
    let index = PackageDir::new(scratch.path(dir)).index_files().unwrap();
    scratch.write(format!("{dir}/{}", FileIndex::PATH), index.bytes());
}

#[test]
fn a_package_is_its_index_and_reads_through_it() {
    let scratch = Scratch::new();
    let package = scratch.path("one");
    scratch.write("one/manifest.toml", "m");
    scratch.write("one/data/a.toml", "ab");
    // The tag, then rows sorted by path bytes: "data/a.toml" before "manifest.toml". Postcard
    // writes the count 2, then each row as its path's length and bytes, its size as a varint, and the 32
    // bytes of its SHA-256; the fingerprint is the SHA-256 of those bytes.
    let list = [
        &b"campfire/package-index/v1"[..],
        &[2, 11],
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
    // With no index, no package.
    assert!(matches!(
        dir.read(),
        Err(ContentError::Missing { path }) if path.as_str() == FileIndex::PATH
    ));
    pack(&scratch, "one");
    assert_eq!(dir.index_files().unwrap().bytes(), list);
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
    let other = PackageDir::in_memory(Arc::new(PackageDir::reindexed(tree)), "pack/other");
    let memory = other.join(Path::new("../one")).read().unwrap();
    assert_eq!(memory.fingerprint(), expected);
    let listed: Vec<_> = memory.files_under("data").collect();
    assert_eq!(listed, [&PackagePath::parse("data/a.toml").unwrap()]);
    let manifest = PackagePath::parse("manifest.toml").unwrap();
    assert_eq!(memory.read_text(&manifest).unwrap(), "m");
    let missing = PackagePath::parse("scripts/x.rhai").unwrap();
    assert!(matches!(
        memory.read_text(&missing),
        Err(ContentError::Missing { .. })
    ));

    // A file the index does not list is no part of the package, until the package is packed
    // again; what was read stays as it was read.
    scratch.write("one/data/b/c.toml", "");
    assert_eq!(dir.read().unwrap().files_under("data").count(), 1);
    assert_eq!(dir.read().unwrap().fingerprint(), expected);
    pack(&scratch, "one");
    let files: Vec<_> = dir
        .read()
        .unwrap()
        .files_under("data")
        .map(PackagePath::to_string)
        .collect();
    assert_eq!(files, ["data/a.toml", "data/b/c.toml"]);
    assert_eq!(read.files_under("data").count(), 1);
    scratch.remove("one/data/b");
    pack(&scratch, "one");

    // A file whose bytes, or whose size, differ from its row fails the read, with its path, and
    // one the index lists that is not there is missing; packed again, it has another
    // fingerprint.
    let data = PackagePath::parse("data/a.toml").unwrap();
    for bytes in ["ac", "abc"] {
        scratch.write("one/data/a.toml", bytes);
        assert!(matches!(dir.read(), Err(ContentError::Changed { path }) if path == data));
    }
    assert_eq!(read.read_text(&data).unwrap(), "ab");
    scratch.write("one/data/a.toml", "ac");
    pack(&scratch, "one");
    let changed = dir.read().unwrap().fingerprint();
    assert_ne!(changed, expected);
    scratch.rename("one/data", "one/date");
    assert!(matches!(dir.read(), Err(ContentError::Missing { path }) if path == data));
    pack(&scratch, "one");
    assert_ne!(dir.read().unwrap().fingerprint(), changed);
}

#[test]
fn a_store_finds_each_package_by_fingerprint() {
    // A store finds each package under a root by fingerprint, and searches no deeper than a
    // manifest. A package that does not read is the store's failure, and costs no other.
    let scratch = Scratch::new();
    let root = scratch.root();
    let dir = PackageDir::new(scratch.path("one"));
    scratch.write("one/manifest.toml", "m");
    scratch.write("nested/two/manifest.toml", "n");
    pack(&scratch, "nested/two");
    scratch.write("one/inner/manifest.toml", "i");
    pack(&scratch, "one/inner");
    pack(&scratch, "one");
    scratch.write("bad/manifest.toml", "b");
    // A manifest is found by its exact name, as a file system that ignores case would not.
    scratch.write("upper/Manifest.toml", "u");
    pack(&scratch, "upper");
    let store = PackageStore::scan(root).unwrap();
    let upper = PackageDir::new(root.join("upper")).read().unwrap();
    assert!(store.get(upper.fingerprint()).is_none());
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
    assert!(matches!(
        &failures[0].error,
        ContentError::Missing { path } if path.as_str() == FileIndex::PATH
    ));
}

#[test]
fn a_package_refuses_what_an_os_would_hold_another_way() {
    let scratch = Scratch::new();
    let package = scratch.path("one");
    scratch.write("one/manifest.toml", "m");
    let dir = PackageDir::new(&package);
    // A link is no file of a package, and a name no package path spells, one past ASCII, is no
    // file of one. Of two flaws, the one refused is the first by name, `a€` before `link`,
    // whatever order the OS lists them in.
    scratch.link("one/manifest.toml", "one/link");
    assert!(
        matches!(dir.index_files(), Err(ContentError::NotAFile(path)) if path == package.join("link"))
    );
    let euro = package.join("a\u{20ac}.toml");
    scratch.write("one/a\u{20ac}.toml", "");
    assert!(matches!(dir.index_files(), Err(ContentError::NotPath(path)) if path == euro));
    scratch.remove("one/link");
    assert!(matches!(dir.index_files(), Err(ContentError::NotPath(path)) if path == euro));
    scratch.remove("one/a\u{20ac}.toml");
    assert!(dir.index_files().is_ok());

    // Two paths, or the directories on their way, that differ only in case are one file where
    // case is ignored, so no package: the index takes them in path order, uppercase first, and
    // refuses the second spelling.
    for (one, two, other) in [
        ("data/a.toml", "data/A.toml", "data/A.toml"),
        ("data/a.toml", "Data/b.toml", "Data"),
    ] {
        let tree: BTreeMap<PathBuf, Vec<u8>> = ["manifest.toml", one, two]
            .into_iter()
            .map(|path| (Path::new("p").join(path), Vec::new()))
            .collect();
        assert!(
            matches!(
                PackageDir::in_memory(Arc::new(tree), "p").index_files(),
                Err(ContentError::CaseClash { path, other: first })
                    if path.as_str() == "data/a.toml" && first.as_str() == other
            ),
            "{one} {two}"
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
    assert!(Toml::parse::<Named>(r#"script = "../x.rhai""#).is_err());
    assert!(matches!(
        walker.read_text(&path("scripts/missing.rhai")),
        Err(ContentError::Missing { .. })
    ));
    assert!(matches!(
        walker.read_data::<Hero>(&path("scripts/strike.rhai")),
        Err(ContentError::Data { .. })
    ));
}

#[test]
fn a_package_keeps_what_a_load_reads_and_reads_the_rest_on_demand() {
    // An asset of 100 000 zeros passes in two chunks of the walk; its row holds its size, the
    // varint 0xA0 0x8D 0x06 (100 000 = 6 × 2¹⁴ + 13 × 2⁷ + 32), and its hash.
    let asset = vec![0_u8; 100_000];
    let tree: BTreeMap<PathBuf, Vec<u8>> = [
        ("one/manifest.toml", b"m".to_vec()),
        ("one/data/a.toml", b"ab".to_vec()),
        ("one/textures/x.png", asset.clone()),
    ]
    .into_iter()
    .map(|(path, bytes)| (PathBuf::from(path), bytes))
    .collect();
    let list = [
        &b"campfire/package-index/v1"[..],
        &[3, 11],
        b"data/a.toml",
        &[2],
        &Sha256::digest(b"ab"),
        &[13],
        b"manifest.toml",
        &[1],
        &Sha256::digest(b"m"),
        &[14],
        b"textures/x.png",
        &[0xA0, 0x8D, 0x06],
        &Sha256::digest(&asset),
    ]
    .concat();
    let expected = Fingerprint::new(Sha256::digest(&list).into());
    let tree = Arc::new(PackageDir::reindexed(tree));
    let read = PackageDir::in_memory(Arc::clone(&tree), "one")
        .read()
        .unwrap();
    assert_eq!(read.fingerprint(), expected);
    // Only the files a load reads stay; an asset reads on demand, checked against its row.
    assert_eq!(read.files_under("textures").count(), 0);
    let texture = PackagePath::parse("textures/x.png").unwrap();
    assert!(matches!(
        read.read_text(&texture),
        Err(ContentError::Missing { .. })
    ));
    assert_eq!(read.read_file(&texture).unwrap(), asset);
    let unlisted = PackagePath::parse("textures/y.png").unwrap();
    assert!(matches!(
        read.read_file(&unlisted),
        Err(ContentError::Missing { path }) if path == unlisted
    ));
    // The same files on disk read the same; an asset changed after the load fails only when it
    // is read, and one that goes is missing only then.
    let scratch = Scratch::new();
    for (path, bytes) in tree.iter() {
        scratch.write(path, bytes);
    }
    let disk = PackageDir::new(scratch.path("one")).read().unwrap();
    assert_eq!(disk.fingerprint(), expected);
    assert_eq!(disk.read_file(&texture).unwrap(), asset);
    scratch.write("one/textures/x.png", [1]);
    assert!(matches!(
        disk.read_file(&texture),
        Err(ContentError::Changed { path }) if path == texture
    ));
    scratch.remove("one/textures/x.png");
    assert!(matches!(
        disk.read_file(&texture),
        Err(ContentError::Missing { path }) if path == texture
    ));

    // A file a load reads that is not UTF-8 reads, but not as text.
    let bad = BTreeMap::from([(PathBuf::from("two/data/bad.toml"), vec![0xFF, b'a'])]);
    let bad = PackageDir::in_memory(Arc::new(bad), "two")
        .index_files()
        .unwrap();
    let tree = BTreeMap::from([
        (PathBuf::from("two/data/bad.toml"), vec![0xFF, b'a']),
        (PathBuf::from("two/package.index"), bad.bytes().to_vec()),
    ]);
    let two = PackageDir::in_memory(Arc::new(tree), "two").read().unwrap();
    let path = PackagePath::parse("data/bad.toml").unwrap();
    assert!(matches!(
        two.read_text(&path),
        Err(ContentError::NotText { path: at, error }) if at == path && error.valid_up_to() == 0
    ));
}

/// The most bytes a test reads of a checked-in index: far past what the workspace's hold.
const INDEX_LEN: usize = 1 << 20;

#[test]
fn every_workspace_package_holds_the_index_of_its_files() {
    fn packages(dir: &Path, found: &mut Vec<PathBuf>) {
        let entries = DirEntries::read(dir).unwrap();
        if entries
            .iter()
            .any(|entry| entry.name == PackageDir::MANIFEST)
        {
            found.push(dir.to_owned());
            return;
        }
        for entry in entries.iter().filter(|entry| entry.kind == EntryKind::Dir) {
            packages(&dir.join(&entry.name), found);
        }
    }
    let mut found = Vec::new();
    packages(&PackageDir::workspace(""), &mut found);
    // The four test heroes, the three test modes, the test rules, the MOBA's six heroes, its
    // spells, its German text and its 3v3, and Zero Hour's rules.
    assert_eq!(found.len(), 18, "{found:?}");
    let bless = env::var_os("CAMPFIRE_BLESS").is_some();
    for dir in found {
        let index = PackageDir::new(&dir).index_files().unwrap();
        let at = dir.join(FileIndex::PATH);
        if bless {
            DurableFile::write(&at, index.bytes()).unwrap();
        }
        let held = InputFile::read_if_present(&at, INDEX_LEN).unwrap();
        assert!(
            held.as_deref() == Some(index.bytes()),
            "{} differs from its files: run this test with CAMPFIRE_BLESS=1",
            at.display()
        );
    }
}
