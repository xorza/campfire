use super::*;

#[test]
fn the_command_line_names_a_mode_and_its_directory() {
    let parse = |args: &[&str]| Mode::parse(args.iter().map(OsString::from));
    let dir = PathBuf::from("run");
    assert_eq!(
        parse(&[]),
        Some(Mode::Play {
            root: env::temp_dir().join("campfire-lan-check")
        })
    );
    assert_eq!(parse(&["run"]), Some(Mode::Play { root: dir.clone() }));
    assert_eq!(parse(&["verify", "run"]), Some(Mode::Verify { dir }));
    for flawed in [
        &["verify"][..],
        &["run", "more"],
        &["verify", "run", "more"],
    ] {
        assert_eq!(parse(flawed), None, "{flawed:?}");
    }
}
