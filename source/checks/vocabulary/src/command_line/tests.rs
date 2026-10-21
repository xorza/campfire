use super::*;

#[test]
fn the_vocabulary_goes_where_the_line_says_or_to_the_target_directory() {
    let line = |args: &[&str]| CommandLine::try_parse_from(args).unwrap();
    let target = Path::new("/t");
    assert_eq!(
        line(&["vocabulary", "/out.txt"]).out_file(target),
        PathBuf::from("/out.txt")
    );
    assert_eq!(
        line(&["vocabulary"]).out_file(target),
        PathBuf::from("/t/vocabulary.txt")
    );
}
