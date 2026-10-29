use std::ffi::OsString;

use clap::error::ErrorKind;

use super::*;

fn read(args: &[&str]) -> Result<CommandLine, clap::Error> {
    CommandLine::try_parse_from(["campfire-verifier"].iter().chain(args).map(OsString::from))
}

#[test]
fn the_command_line_names_a_verification_a_show_or_a_report_or_its_flaw() {
    let verify = |args: &[&str]| match read(args).unwrap().command {
        VerifierCommand::Verify {
            packages,
            log,
            snapshots,
        } => (packages, log, snapshots),
        VerifierCommand::Show { .. } | VerifierCommand::Report { .. } => {
            panic!("{args:?} names a verification")
        }
    };
    assert_eq!(
        verify(&["verify", "packages", "session.log", "snapshots"]),
        (
            PathBuf::from("packages"),
            PathBuf::from("session.log"),
            Some(PathBuf::from("snapshots"))
        )
    );
    assert_eq!(verify(&["verify", "packages", "session.log"]).2, None);

    // A show of a log.
    let VerifierCommand::Show { log } = read(&["show", "session.log"]).unwrap().command else {
        panic!("`show` names a show");
    };
    assert_eq!(log, PathBuf::from("session.log"));

    // A report, with the probes' directory the line names or `target/report`.
    let report = |args: &[&str]| match read(args).unwrap().command {
        VerifierCommand::Report {
            report,
            probe,
            probe_dir,
        } => (report, probe, probe_dir),
        VerifierCommand::Verify { .. } | VerifierCommand::Show { .. } => {
            panic!("{args:?} names a report")
        }
    };
    let spec: ProbeSpec = "1000:all".parse().unwrap();
    assert_eq!(
        report(&["report", "reports/a-40", "--probe", "1000:all"]),
        (
            PathBuf::from("reports/a-40"),
            Some(spec),
            PathBuf::from("target/report")
        )
    );
    let probes = report(&["report", "reports/a-40", "--probe-dir", "probes"]);
    assert_eq!((probes.1, probes.2), (None, PathBuf::from("probes")));

    for (args, kind) in [
        (
            &["packages", "session.log"][..],
            ErrorKind::InvalidSubcommand,
        ),
        (&["verify", "packages"], ErrorKind::MissingRequiredArgument),
        (
            &["verify", "packages", "session.log", "snapshots", "more"],
            ErrorKind::UnknownArgument,
        ),
        (
            &["verify", "--fast", "packages", "session.log"],
            ErrorKind::UnknownArgument,
        ),
        (&["report"], ErrorKind::MissingRequiredArgument),
        (&["show"], ErrorKind::MissingRequiredArgument),
        (
            &["report", "reports/a-40", "--probe", "1000"],
            ErrorKind::ValueValidation,
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
