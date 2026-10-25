use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_common::PackageFingerprint;
use campfire_package_format::ContentHash;
use campfire_package_format::PackageFinder;
use campfire_store::internals::{PinnedFile, TestDir};

use super::*;
use crate::package_store::PackageStore;
use crate::tree_dir::TreeDir;

/// Writes the index of the package at `dir`, under `scratch`, from its files.
fn pack(test_dir: &TestDir, dir: &str) {
    let index = PackageDir::new(test_dir.path(dir)).index_files().unwrap();
    test_dir.write(format!("{dir}/{}", FileIndex::PATH), index.bytes);
}

#[test]
fn a_package_is_its_index_and_reads_through_it() {
    let test_dir = TestDir::new();
    let package = test_dir.path("one");
    test_dir.write("one/manifest.toml", "m");
    test_dir.write("one/data/a.toml", "ab");
    // The tag's line, then a line for each row sorted by path bytes, "data/a.toml" before
    // "manifest.toml": its SHA-256 in hex, its size and its path; the fingerprint is the SHA-256
    // of those bytes.
    let hex = ContentHash::of;
    let list = format!(
        "campfire/package-index/v2\n{} 2 data/a.toml\n{} 1 manifest.toml\n",
        hex(b"ab"),
        hex(b"m")
    )
    .into_bytes();
    let expected = PackageFingerprint::of(&list);
    let dir = PackageDir::new(&package);
    // With no index, no package.
    assert!(matches!(
        dir.files(),
        Err(ContentError::Missing { path }) if path.as_str() == FileIndex::PATH
    ));
    pack(&test_dir, "one");
    assert_eq!(dir.index_files().unwrap().bytes, list);
    let read = dir.files().unwrap();
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
    let other = TreeDir::new(Arc::new(TreeDir::reindexed(tree)), "pack/other");
    let memory = other
        .join(&DependencyPath::parse("../one").unwrap())
        .files()
        .unwrap();
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
    test_dir.write("one/data/b/c.toml", "");
    assert_eq!(dir.files().unwrap().files_under("data").count(), 1);
    assert_eq!(dir.files().unwrap().fingerprint(), expected);
    pack(&test_dir, "one");
    let files: Vec<_> = dir
        .files()
        .unwrap()
        .files_under("data")
        .map(PackagePath::to_string)
        .collect();
    assert_eq!(files, ["data/a.toml", "data/b/c.toml"]);
    assert_eq!(read.files_under("data").count(), 1);
    test_dir.remove("one/data/b");
    pack(&test_dir, "one");

    // A file whose bytes, or whose size, differ from its row fails the read, with its path, and
    // one the index lists that is not there is missing; packed again, it has another
    // fingerprint.
    let data = PackagePath::parse("data/a.toml").unwrap();
    for bytes in ["ac", "abc"] {
        test_dir.write("one/data/a.toml", bytes);
        assert!(matches!(dir.files(), Err(ContentError::Changed { path }) if path == data));
    }
    assert_eq!(read.read_text(&data).unwrap(), "ab");
    test_dir.write("one/data/a.toml", "ac");
    pack(&test_dir, "one");
    let changed = dir.files().unwrap().fingerprint();
    assert_ne!(changed, expected);
    // A file or a directory whose name on disk differs from its row's only in case fails the
    // open as missing on every OS, as Linux holds it, though a file system that ignores case
    // would open it.
    for (from, to) in [
        ("one/data/a.toml", "one/data/A.toml"),
        ("one/data", "one/Data"),
    ] {
        test_dir.rename(from, to);
        assert!(matches!(dir.files(), Err(ContentError::Missing { path }) if path == data));
        test_dir.rename(to, from);
    }
    assert_eq!(dir.files().unwrap().fingerprint(), changed);
    test_dir.rename("one/data", "one/date");
    assert!(matches!(dir.files(), Err(ContentError::Missing { path }) if path == data));
    pack(&test_dir, "one");
    assert_ne!(dir.files().unwrap().fingerprint(), changed);
}

#[test]
fn a_store_finds_each_package_by_fingerprint() {
    // A store finds each package under a root by fingerprint, and searches no deeper than a
    // manifest. A package that does not read is the store's failure, and costs no other.
    let test_dir = TestDir::new();
    let root = test_dir.root();
    let dir = PackageDir::new(test_dir.path("one"));
    test_dir.write("one/manifest.toml", "m");
    test_dir.write("nested/two/manifest.toml", "n");
    pack(&test_dir, "nested/two");
    test_dir.write("one/inner/manifest.toml", "i");
    pack(&test_dir, "one/inner");
    pack(&test_dir, "one");
    test_dir.write("bad/manifest.toml", "b");
    // A manifest is found by its exact name, as a file system that ignores case would not.
    test_dir.write("upper/Manifest.toml", "u");
    pack(&test_dir, "upper");
    let store = PackageStore::scan(root).unwrap();
    let upper = PackageDir::new(root.join("upper")).files().unwrap();
    assert!(!store.holds(upper.fingerprint()));
    let one = dir.files().unwrap().fingerprint();
    let two = PackageDir::new(root.join("nested/two"))
        .files()
        .unwrap()
        .fingerprint();
    for fingerprint in [one, two] {
        assert!(store.holds(fingerprint));
    }
    let inner = PackageDir::new(root.join("one/inner"))
        .files()
        .unwrap()
        .fingerprint();
    assert!(!store.holds(inner));
    let failures = store.failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].dir, root.join("bad"));
    assert!(matches!(
        &failures[0].error,
        ContentError::Missing { path } if path.as_str() == FileIndex::PATH
    ));
    // The scan read each index alone: a file changed after it fails only the read of its package,
    // as a log names it.
    test_dir.write("nested/two/manifest.toml", "changed");
    assert!(store.holds(two));
    assert!(matches!(
        store.find(two),
        Some(Err(ContentError::Changed { path })) if path.as_str() == PackageFiles::MANIFEST
    ));
    assert!(store.find(inner).is_none());
}

#[test]
fn a_package_refuses_what_an_os_would_hold_another_way() {
    let test_dir = TestDir::new();
    let package = test_dir.path("one");
    test_dir.write("one/manifest.toml", "m");
    let dir = PackageDir::new(&package);
    // A link is no file of a package, and a name no package path spells, one past ASCII, is no
    // file of one. Of two flaws, the one refused is the first by name, `a€` before `link`,
    // whatever order the OS lists them in.
    test_dir.link("one/manifest.toml", "one/link");
    assert!(
        matches!(dir.index_files(), Err(IndexError::NotAFile(path)) if path == package.join("link"))
    );
    let euro = package.join("a\u{20ac}.toml");
    test_dir.write("one/a\u{20ac}.toml", "");
    assert!(matches!(dir.index_files(), Err(IndexError::NotPath(path)) if path == euro));
    test_dir.remove("one/link");
    assert!(matches!(dir.index_files(), Err(IndexError::NotPath(path)) if path == euro));
    test_dir.remove("one/a\u{20ac}.toml");
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
                TreeDir::new(Arc::new(tree), "p").index_files(),
                Err(IndexError::Package(ContentError::CaseClash { path, other: first }))
                    if path.as_str() == "data/a.toml" && first.as_str() == other
            ),
            "{one} {two}"
        );
    }
}

fn walker() -> PackageDir {
    PackageDir::new(PackageDir::workspace("test/engine/heroes/walker"))
}

#[test]
fn a_package_reads_its_own_files_only() {
    #[derive(Debug, serde::Deserialize)]
    struct Hero {
        name: String,
        slots: BTreeMap<String, Vec<String>>,
        actions: BTreeMap<String, Action>,
    }
    #[derive(Debug, serde::Deserialize)]
    struct Action {
        name: String,
    }
    let path = |text| PackagePath::parse(text).unwrap();
    let walker = walker().files().unwrap();
    let hero: Hero = walker.read_data(&path("data/avatar.toml")).unwrap();
    assert_eq!(hero.name, "hero-name");
    assert_eq!(hero.slots["basic"][2], "third");
    assert_eq!(hero.actions["first"].name, "action-first");
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
    // An asset of 100 000 zeros passes in two chunks of the walk; its row holds its size and its
    // hash.
    let asset = vec![0_u8; 100_000];
    let tree: BTreeMap<PathBuf, Vec<u8>> = [
        ("one/manifest.toml", b"m".to_vec()),
        ("one/data/a.toml", b"ab".to_vec()),
        ("one/textures/x.png", asset.clone()),
    ]
    .into_iter()
    .map(|(path, bytes)| (PathBuf::from(path), bytes))
    .collect();
    let hex = ContentHash::of;
    let list = format!(
        "campfire/package-index/v2\n{} 2 data/a.toml\n{} 1 manifest.toml\n{} 100000 \
         textures/x.png\n",
        hex(b"ab"),
        hex(b"m"),
        hex(&asset)
    )
    .into_bytes();
    let expected = PackageFingerprint::of(&list);
    let tree = Arc::new(TreeDir::reindexed(tree));
    let read = TreeDir::new(Arc::clone(&tree), "one").files().unwrap();
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
    // No read takes a file whole: a read of a row's size refuses a file of another, as
    // `ReadBound` holds.
    let one = TreeDir::new(Arc::clone(&tree), "one");
    let len = u64::try_from(asset.len()).unwrap();
    assert_eq!(one.read(&texture, len).unwrap(), asset);
    assert!(matches!(
        one.read(&texture, len + 1),
        Err(ContentError::Changed { path }) if path == texture
    ));
    // The same files on disk read the same; an asset changed after the load fails only when it
    // is read, and one that goes is missing only then.
    let test_dir = TestDir::new();
    for (path, bytes) in tree.iter() {
        test_dir.write(path, bytes);
    }
    // A file a load does not read fails the open too when it is not on disk by its row's exact
    // name, or not there at all: a package is whole.
    let one = PackageDir::new(test_dir.path("one"));
    test_dir.rename("one/textures/x.png", "one/textures/X.png");
    assert!(matches!(one.files(), Err(ContentError::Missing { path }) if path == texture));
    test_dir.rename("one/textures/X.png", "one/textures/x.png");
    let disk = one.files().unwrap();
    assert_eq!(disk.fingerprint(), expected);
    assert_eq!(disk.read_file(&texture).unwrap(), asset);
    test_dir.write("one/textures/x.png", [1]);
    assert!(matches!(
        disk.read_file(&texture),
        Err(ContentError::Changed { path }) if path == texture
    ));
    test_dir.remove("one/textures/x.png");
    assert!(matches!(
        disk.read_file(&texture),
        Err(ContentError::Missing { path }) if path == texture
    ));
    assert!(matches!(one.files(), Err(ContentError::Missing { path }) if path == texture));

    // A file a load reads that is not UTF-8 reads, but not as text.
    let bad = BTreeMap::from([(PathBuf::from("two/data/bad.toml"), vec![0xFF, b'a'])]);
    let bad = TreeDir::new(Arc::new(bad), "two").index_files().unwrap();
    let tree = BTreeMap::from([
        (PathBuf::from("two/data/bad.toml"), vec![0xFF, b'a']),
        (PathBuf::from("two/package.index"), bad.bytes),
    ]);
    let two = TreeDir::new(Arc::new(tree), "two").files().unwrap();
    let path = PackagePath::parse("data/bad.toml").unwrap();
    assert!(matches!(
        two.read_text(&path),
        Err(ContentError::NotText { path: at, error }) if at == path && error.valid_up_to() == 0
    ));
}

#[test]
fn a_row_reads_from_the_first_path_of_its_sha256_that_the_package_holds() {
    // Three paths of the bytes "x", in the index's order `data/a.toml`, `data/b.toml`,
    // `textures/c.png`, indexed from a tree that holds each, then held at fewer paths.
    let path = |text| PackagePath::parse(text).unwrap();
    let (a, b, c) = (
        path("data/a.toml"),
        path("data/b.toml"),
        path("textures/c.png"),
    );
    let full: BTreeMap<PathBuf, Vec<u8>> = [
        ("p/manifest.toml", "m"),
        ("p/data/a.toml", "x"),
        ("p/data/b.toml", "x"),
        ("p/textures/c.png", "x"),
    ]
    .into_iter()
    .map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec()))
    .collect();
    let full = TreeDir::reindexed(full);
    let held = |keep: &[&str]| {
        let mut tree = full.clone();
        tree.retain(|file, _| {
            let file = file.to_str().unwrap();
            !["p/data/a.toml", "p/data/b.toml", "p/textures/c.png"].contains(&file)
                || keep.contains(&file)
        });
        TreeDir::new(Arc::new(tree), "p").files()
    };
    let fingerprint = held(&["p/data/a.toml", "p/data/b.toml", "p/textures/c.png"])
        .unwrap()
        .fingerprint();
    // The first path alone: every row reads its bytes, those a load keeps and one on demand.
    let first = held(&["p/data/a.toml"]).unwrap();
    assert_eq!(first.fingerprint(), fingerprint);
    assert_eq!(
        [&a, &b].map(|path| first.read_text(path).unwrap()),
        ["x", "x"]
    );
    assert_eq!(first.read_file(&c).unwrap(), b"x");
    // The first path missing and the second held: every row reads from the second.
    let second = held(&["p/data/b.toml"]).unwrap();
    assert_eq!(second.read_text(&a).unwrap(), "x");
    assert_eq!(second.read_file(&c).unwrap(), b"x");
    // No path held: the open fails on the first row of those bytes, by its own path.
    assert!(matches!(held(&[]), Err(ContentError::Missing { path }) if path == a));

    // On disk the same, by exact names: a copy at the second path that differs is not read, as
    // the first holds the bytes; a path held only in another case is not held.
    let test_dir = TestDir::new();
    for (file, bytes) in &full {
        test_dir.write(file, bytes);
    }
    let dir = PackageDir::new(test_dir.path("p"));
    test_dir.write("p/data/b.toml", "y");
    assert_eq!(dir.files().unwrap().read_text(&b).unwrap(), "x");
    test_dir.remove("p/data/b.toml");
    test_dir.remove("p/textures/c.png");
    test_dir.rename("p/data/a.toml", "p/data/A.toml");
    assert!(matches!(dir.files(), Err(ContentError::Missing { path }) if path == a));
    test_dir.rename("p/data/A.toml", "p/data/a.toml");
    let disk = dir.files().unwrap();
    assert_eq!(disk.fingerprint(), fingerprint);
    assert_eq!(disk.read_file(&c).unwrap(), b"x");
    // A read on demand of a row held elsewhere fails by the path it reads, once that file goes.
    test_dir.remove("p/data/a.toml");
    assert!(matches!(disk.read_file(&c), Err(ContentError::Missing { path }) if path == a));
    // A directory or a link at the first path holds no bytes: the second path's file does.
    test_dir.write("p/data/b.toml", "x");
    test_dir.write("p/data/a.toml/inside", "x");
    assert_eq!(dir.files().unwrap().read_text(&a).unwrap(), "x");
    test_dir.remove("p/data/a.toml");
    test_dir.link("p/manifest.toml", "p/data/a.toml");
    assert_eq!(dir.files().unwrap().read_text(&a).unwrap(), "x");
}

/// The most bytes a test reads of a checked-in index: far past what the workspace's hold.
const INDEX_LEN: usize = 1 << 20;

#[test]
fn every_workspace_package_holds_the_index_of_its_files() {
    fn packages(dir: &Path, found: &mut Vec<PathBuf>) {
        let entries = DirEntries::read(dir).unwrap();
        if entries
            .iter()
            .any(|entry| entry.name == PackageFiles::MANIFEST)
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
    // The six test heroes, the six test modes, the test spells and rules, the MOBA's six heroes,
    // its spells, its German text and its 3v3, and Zero Hour's rules.
    assert_eq!(found.len(), 24, "{found:?}");
    for dir in found {
        let index = PackageDir::new(&dir).index_files().unwrap();
        let at = dir.join(FileIndex::PATH);
        PinnedFile::new(&at).bless(&index.bytes).unwrap();
        let held = InputFile::read_if_present(&at, INDEX_LEN).unwrap();
        assert!(
            held.as_deref() == Some(index.bytes.as_slice()),
            "{} differs from its files: write it again with `cargo drive run index`",
            at.display()
        );
    }
}
