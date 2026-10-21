use campfire_store::internals::TestDir;

use super::*;
use crate::package_dir::PackageDir;

fn path(text: &str) -> PackagePath {
    PackagePath::parse(text).unwrap()
}

#[test]
fn a_writer_writes_each_file_and_their_index_in_path_order() {
    let test_dir = TestDir::new();
    let write = |dir: &str, order: [(&str, &[u8]); 2]| {
        let mut writer = PackageWriter::create(&test_dir.path(dir)).unwrap();
        for (at, bytes) in order {
            writer.write(path(at), bytes).unwrap();
        }
        writer.finish().unwrap()
    };
    let fingerprint = write("one", [("data/b.toml", b"b"), ("a.txt", b"aa")]);
    assert_eq!(test_dir.read("one/data/b.toml"), b"b");
    assert_eq!(test_dir.read("one/a.txt"), b"aa");
    // The tag's line, then the rows in path order: `a.txt` before `data/b.toml`.
    let hex = ContentHash::of;
    let index = format!(
        "campfire/package-index/v2\n{} 2 a.txt\n{} 1 data/b.toml\n",
        hex(b"aa"),
        hex(b"b")
    )
    .into_bytes();
    assert_eq!(test_dir.read("one/package.index"), index);
    assert_eq!(fingerprint, PackageFingerprint::of(&index));
    assert_eq!(
        PackageDir::new(test_dir.path("one"))
            .files()
            .unwrap()
            .fingerprint(),
        fingerprint
    );
    // The same files written in another order are the same package.
    assert_eq!(
        write("two", [("a.txt", b"aa"), ("data/b.toml", b"b")]),
        fingerprint
    );
    assert_eq!(test_dir.read("two/package.index"), index);

    // A file of the bytes of one written before is a link to that file, the first of them in
    // the order of writing: one file on disk, which each path reads whole; the index lists
    // each path, as a writer of copies would.
    let mut linked = PackageWriter::create(&test_dir.path("four")).unwrap();
    for (at, bytes) in [
        ("data/b.toml", &b"b"[..]),
        ("a.txt", b"aa"),
        ("c/d.txt", b"aa"),
    ] {
        linked.write(path(at), bytes).unwrap();
    }
    let fingerprint = linked.finish().unwrap();
    assert_eq!(test_dir.read("four/c/d.txt"), b"aa");
    assert!(test_dir.same_file("four/a.txt", "four/c/d.txt"));
    assert!(!test_dir.same_file("four/a.txt", "four/data/b.toml"));
    let rows = format!(
        "campfire/package-index/v2\n{aa} 2 a.txt\n{aa} 2 c/d.txt\n{} 1 data/b.toml\n",
        hex(b"b"),
        aa = hex(b"aa")
    );
    assert_eq!(test_dir.read("four/package.index"), rows.as_bytes());
    let files = PackageDir::new(test_dir.path("four")).files().unwrap();
    assert_eq!(files.fingerprint(), fingerprint);

    // A path written twice, of other bytes or the same, and a directory that is there, are
    // refused.
    let mut twice = PackageWriter::create(&test_dir.path("three")).unwrap();
    twice.write(path("a.txt"), b"1").unwrap();
    for bytes in [b"2", b"1"] {
        assert!(matches!(
            twice.write(path("a.txt"), bytes),
            Err(WriteError::File(error)) if error.path == test_dir.path("three/a.txt")
        ));
    }
    // The refused writes keep no row and no link's target: a file of the bytes of the
    // refused `2` is a file of its own, and the index lists `a.txt` as its first write.
    twice.write(path("b.txt"), b"2").unwrap();
    assert!(!test_dir.same_file("three/a.txt", "three/b.txt"));
    assert_eq!(test_dir.read("three/b.txt"), b"2");
    twice.finish().unwrap();
    let rows = format!(
        "campfire/package-index/v2\n{} 1 a.txt\n{} 1 b.txt\n",
        hex(b"1"),
        hex(b"2")
    );
    assert_eq!(test_dir.read("three/package.index"), rows.as_bytes());
    assert!(matches!(
        PackageWriter::create(&test_dir.path("one")),
        Err(WriteError::CreateDir(_))
    ));
}
