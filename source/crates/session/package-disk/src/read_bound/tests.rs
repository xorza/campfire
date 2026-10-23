use super::*;

#[test]
fn a_read_takes_no_more_than_its_bound() {
    let path = PackagePath::parse("textures/x.png").unwrap();
    let file = Path::new("one/textures/x.png");
    // An upper bound takes a file of its length or less, as the index's read takes at most
    // `FileIndex::MAX_LEN`, and refuses one byte more; an exact bound takes its size alone.
    let at_most = ReadBound::AtMost(5);
    assert_eq!(at_most.take(5, &path, file).unwrap(), 5);
    assert_eq!(at_most.take(0, &path, file).unwrap(), 0);
    assert!(matches!(
        at_most.take(6, &path, file),
        Err(ContentError::Read(error)) if error.to_string() == file.display().to_string()
    ));
    let exact = ReadBound::Exact(5);
    assert_eq!(exact.take(5, &path, file).unwrap(), 5);
    for len in [4, 6] {
        assert!(matches!(
            exact.take(len, &path, file),
            Err(ContentError::Changed { path: changed }) if changed == path
        ));
    }
}
