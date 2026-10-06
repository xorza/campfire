use std::error::Error;
use std::ffi::OsString;

use campfire_common::PlayerSlot;
use campfire_net::SlotBotFileError;
use clap::error::ErrorKind;

use super::*;

fn read(args: &[&str]) -> Result<Args, clap::Error> {
    CommandLine::try_parse_from(["campfire-server"].iter().chain(args).map(OsString::from))
        .map(Args::of)
}

#[test]
fn the_command_line_reads_its_flags_in_any_order_or_its_flaw() {
    let args = read(&[
        "--server-bot",
        "1=bot.toml",
        "--grace",
        "5",
        "--data",
        "d",
        "mode",
        "--takeover",
        "takeover.toml",
        "0.0.0.0:4433",
        "--restore-window",
        "9",
    ])
    .unwrap();
    assert_eq!(args.data, PathBuf::from("d"));
    assert_eq!(
        (args.times.grace, args.times.restore_window),
        (Duration::from_secs(5), Duration::from_secs(9))
    );
    let bots: Vec<(PlayerSlot, PathBuf)> = args
        .bots
        .into_iter()
        .map(|bot| (bot.slot, bot.path))
        .collect();
    assert_eq!(bots, [(PlayerSlot::new(1), PathBuf::from("bot.toml"))]);
    assert_eq!(args.takeover, Some(PathBuf::from("takeover.toml")));
    assert_eq!(args.mode, PathBuf::from("mode"));
    assert_eq!(args.address, SocketAddr::from(([0, 0, 0, 0], 4433)));
    // The defaults, with no flag but `--data`.
    let plain = read(&["--data", "d", "mode", "0.0.0.0:4433"]).unwrap();
    assert_eq!(plain.times, SessionTimes::DEFAULT);
    assert!(plain.bots.is_empty() && plain.takeover.is_none());

    let refused = |args: &[&str]| read(args).unwrap_err();
    for (args, kind) in [
        (
            &["--data", "d", "--bot", "1=bot.toml", "mode", "0.0.0.0:4433"][..],
            ErrorKind::UnknownArgument,
        ),
        (
            &["--data", "d", "--data", "e", "mode", "0.0.0.0:4433"],
            ErrorKind::ArgumentConflict,
        ),
        (
            &["mode", "0.0.0.0:4433"],
            ErrorKind::MissingRequiredArgument,
        ),
        (&["--data"], ErrorKind::InvalidValue),
        (&["--data", "d", "mode"], ErrorKind::MissingRequiredArgument),
        (
            &["--data", "d", "mode", "0.0.0.0:4433", "x"],
            ErrorKind::UnknownArgument,
        ),
    ] {
        assert_eq!(refused(args).kind(), kind, "{args:?}");
    }

    // A value its parser refuses gives that parser's error as the source.
    let invalid = |args: &[&str]| {
        let error = refused(args);
        assert_eq!(error.kind(), ErrorKind::ValueValidation, "{args:?}");
        error
    };
    let source = |args: &[&str]| invalid(args).source().map(ToString::to_string);
    assert_eq!(
        source(&["--data", "d", "--grace", "soon", "mode", "0.0.0.0:4433"]).as_deref(),
        Some("invalid digit found in string")
    );
    assert_eq!(
        source(&["--data", "d", "mode", "nowhere"]).as_deref(),
        Some("invalid socket address syntax")
    );
    let bot = invalid(&["--data", "d", "--server-bot", "1", "mode", "0.0.0.0:4433"]);
    assert_eq!(
        bot.source().and_then(|error| error.downcast_ref()),
        Some(&SlotBotFileError::NotPair("1".to_owned()))
    );

    // The help is no refusal: clap writes it to standard output, and the server exits with
    // success.
    let help = refused(&["--help"]);
    assert_eq!(help.kind(), ErrorKind::DisplayHelp);
    assert!(!help.use_stderr() && help.exit_code() == 0);
}
