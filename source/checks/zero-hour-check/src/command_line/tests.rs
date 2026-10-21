use super::*;

#[test]
fn the_packages_go_where_the_line_says_or_to_the_home_directory() {
    let line = |args: &[&str]| CommandLine::try_parse_from(args).unwrap();
    let home = || Some(PathBuf::from("/h"));
    assert_eq!(
        line(&["check", "/out"]).out_dir(home()),
        Some(PathBuf::from("/out"))
    );
    assert_eq!(
        line(&["check"]).out_dir(home()),
        Some(PathBuf::from("/h/zh-package"))
    );
    assert_eq!(line(&["check"]).out_dir(None), None);
}
