use std::ffi::OsString;

use clap::error::ErrorKind;

use super::*;

fn read(args: &[&str]) -> Result<Args, clap::Error> {
    CommandLine::try_parse_from(
        ["campfire-lan-check"]
            .iter()
            .chain(args)
            .map(OsString::from),
    )
    .map(Args::of)
}

#[test]
fn the_command_line_names_a_mode_and_its_directory() {
    let dir = PathBuf::from("run");
    assert_eq!(read(&["run"]).unwrap(), Args::Play { root: dir.clone() });
    assert_eq!(
        read(&["verify", "run"]).unwrap(),
        Args::Verify {
            dir: dir.clone(),
            verifier: None
        }
    );
    let verifier = Some(PathBuf::from("bin/campfire-verifier"));
    for args in [
        &["verify", "--verifier", "bin/campfire-verifier", "run"],
        &["verify", "run", "--verifier", "bin/campfire-verifier"],
    ] {
        assert_eq!(
            read(args).unwrap(),
            Args::Verify {
                dir: dir.clone(),
                verifier: verifier.clone()
            },
            "{args:?}"
        );
    }
    // `verify` alone names the subcommand: `help` is a run root, as clap's help is a flag.
    assert_eq!(
        read(&["help"]).unwrap(),
        Args::Play {
            root: PathBuf::from("help")
        }
    );
    // A second argument after a run root is taken as a subcommand, which a root refuses.
    for (args, kind) in [
        (&[][..], ErrorKind::MissingRequiredArgument),
        (&["verify"], ErrorKind::MissingRequiredArgument),
        (&["run", "more"], ErrorKind::ArgumentConflict),
        (&["run", "verify", "other"], ErrorKind::ArgumentConflict),
        (&["verify", "run", "more"], ErrorKind::UnknownArgument),
        // The verifier is an option of `verify` alone, and names a file.
        (
            &["--verifier", "bin/campfire-verifier"],
            ErrorKind::UnknownArgument,
        ),
        (&["verify", "run", "--verifier"], ErrorKind::InvalidValue),
        (&["--fast"], ErrorKind::UnknownArgument),
    ] {
        assert_eq!(read(args).unwrap_err().kind(), kind, "{args:?}");
    }

    // The help is no refusal: clap writes it to standard output, and the check exits with
    // success.
    let help = read(&["--help"]).unwrap_err();
    assert_eq!(help.kind(), ErrorKind::DisplayHelp);
    assert!(!help.use_stderr() && help.exit_code() == 0);
}
