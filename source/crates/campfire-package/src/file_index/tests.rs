use super::*;

/// An index's bytes: its tag, then `list`.
fn tagged(list: &[u8]) -> Vec<u8> {
    [INDEX_TAG, list].concat()
}

/// One row as postcard writes it: its path's length and bytes, its size as a varint, and the 32
/// bytes of its SHA-256.
fn row(path: &str, size: u8, sha256: [u8; 32]) -> Vec<u8> {
    [
        &[u8::try_from(path.len()).unwrap()][..],
        path.as_bytes(),
        &[size],
        &sha256,
    ]
    .concat()
}

#[test]
fn an_index_is_its_one_encoding_of_rows_in_path_order() {
    let a = row("data/a.toml", 2, [1; 32]);
    let m = row("manifest.toml", 1, [2; 32]);
    // Two rows in order: the count 2, then each row.
    let bytes = tagged(&[&[2][..], &a, &m].concat());
    let index = FileIndex::decode(&bytes).unwrap();
    assert_eq!(index.bytes(), bytes);
    assert_eq!(
        index.fingerprint(),
        Fingerprint::new(Sha256::digest(&bytes).into())
    );
    let data = PackagePath::parse("data/a.toml").unwrap();
    assert_eq!(
        index.row(&data),
        Some(FileRow {
            size: 2,
            sha256: [1; 32]
        })
    );
    assert_eq!(index.row(&PackagePath::parse("data/b.toml").unwrap()), None);
    // No rows is a package of no file.
    assert_eq!(FileIndex::decode(&tagged(&[0])).unwrap().rows().count(), 0);
    // No tag, or another version's: no index of this version.
    for bytes in [vec![0], [&b"campfire/package-index/v2"[..], &[0]].concat()] {
        assert!(matches!(
            FileIndex::decode(&bytes),
            Err(ContentError::IndexTag)
        ));
    }

    // Out of order, or the same path twice: the second row is refused.
    for bytes in
        [[&[2][..], &m, &a].concat(), [&[2][..], &a, &a].concat()].map(|list| tagged(&list))
    {
        assert!(matches!(
            FileIndex::decode(&bytes),
            Err(ContentError::IndexOrder(_))
        ));
    }
    // A count past its rows, a row cut short: no list.
    for bytes in [vec![1], [&[1][..], &a[..5]].concat()].map(|list| tagged(&list)) {
        assert!(matches!(
            FileIndex::decode(&bytes),
            Err(ContentError::IndexDecode(_))
        ));
    }
    // The count 0 as an overlong varint, and bytes past the list: not the one encoding.
    for bytes in [vec![0x80, 0x00], vec![0, 0]].map(|list| tagged(&list)) {
        assert!(matches!(
            FileIndex::decode(&bytes),
            Err(ContentError::IndexNotCanonical)
        ));
    }
    // A path no package names, the index's own path, and two that differ only in case.
    let refused =
        |path: &str| FileIndex::decode(&tagged(&[&[1][..], &row(path, 0, [0; 32])].concat()));
    assert!(matches!(refused("../x"), Err(ContentError::IndexPath(path)) if path == "../x"));
    assert!(matches!(
        refused("package.index"),
        Err(ContentError::IndexListsItself)
    ));
    let clash = [
        &[2][..],
        &row("Data/b", 0, [0; 32]),
        &row("data/a", 0, [0; 32]),
    ]
    .concat();
    assert!(matches!(
        FileIndex::decode(&tagged(&clash)),
        Err(ContentError::CaseClash { path, other })
            if path.as_str() == "data/a" && other.as_str() == "Data"
    ));
}
