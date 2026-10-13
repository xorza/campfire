use std::ffi::OsString;

use clap::error::ErrorKind;

use super::*;

fn read(args: &[&str]) -> Result<Args, clap::Error> {
    Args::try_parse_from(["campfire-verifier"].iter().chain(args).map(OsString::from))
}

#[test]
fn the_command_line_names_the_packages_the_log_and_the_snapshots_or_its_flaw() {
    let args = read(&["packages", "session.log", "snapshots"]).unwrap();
    assert_eq!(
        (args.packages, args.log, args.snapshots),
        (
            PathBuf::from("packages"),
            PathBuf::from("session.log"),
            Some(PathBuf::from("snapshots"))
        )
    );
    assert_eq!(read(&["packages", "session.log"]).unwrap().snapshots, None);

    for (args, kind) in [
        (&["packages"][..], ErrorKind::MissingRequiredArgument),
        (
            &["packages", "session.log", "snapshots", "more"],
            ErrorKind::UnknownArgument,
        ),
        (
            &["--fast", "packages", "session.log"],
            ErrorKind::UnknownArgument,
        ),
    ] {
        assert_eq!(read(args).unwrap_err().kind(), kind, "{args:?}");
    }

    // The help is no refusal: clap writes it to standard output, and the verifier exits with
    // success.
    let help = read(&["--help"]).unwrap_err();
    assert_eq!(help.kind(), ErrorKind::DisplayHelp);
    assert!(!help.use_stderr() && help.exit_code() == 0);
}
