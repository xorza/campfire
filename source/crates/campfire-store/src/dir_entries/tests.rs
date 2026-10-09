use super::*;
use crate::scratch::Scratch;

#[test]
fn a_listing_gives_each_entry_by_its_names_bytes_with_its_kind_a_link_not_followed() {
    // `B` sorts before `a` by its byte, 0x42 before 0x61, on every OS; the link names a file,
    // and is a link.
    let scratch = Scratch::new();
    scratch.write("dir/a", b"");
    scratch.create_dir("dir/B");
    scratch.link("dir/a", "dir/link");
    let entry = |name: &str, kind| DirEntry {
        name: OsString::from(name),
        kind,
    };
    assert_eq!(
        DirEntries::read(&scratch.path("dir")).unwrap(),
        [
            entry("B", EntryKind::Dir),
            entry("a", EntryKind::File),
            entry("link", EntryKind::Link),
        ]
    );
    assert_eq!(DirEntries::read(&scratch.path("dir/B")).unwrap(), []);
    let refused = |relative: &str| DirEntries::read(&scratch.path(relative)).unwrap_err();
    assert!(matches!(refused("none").error, ReadError::Missing));
    assert!(matches!(refused("dir/a").error, ReadError::NotDir));
    assert_eq!(refused("none").path, scratch.path("none"));
}
