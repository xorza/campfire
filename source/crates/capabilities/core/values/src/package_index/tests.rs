use super::*;

#[test]
fn the_mode_is_0_and_the_last_dependency_is_the_largest_index() {
    assert_eq!(PackageIndex::MODE.get(), 0);
    assert_eq!(PackageIndex::dependency(0).map(PackageIndex::get), Some(1));
    let last = usize::from(u16::MAX) - 1;
    assert_eq!(
        PackageIndex::dependency(last).map(PackageIndex::get),
        Some(u16::MAX)
    );
    assert_eq!(PackageIndex::dependency(last + 1), None);
    assert_eq!(PackageIndex::dependency(usize::MAX), None);
}
