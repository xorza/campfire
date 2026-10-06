use campfire_log::ErrorReport;

use super::*;

fn parse(args: &[&str]) -> Result<Args, ArgsError> {
    Args::parse(args.iter().map(OsString::from))
}

#[test]
fn the_command_line_reads_its_flags_in_any_order_or_its_flaw() {
    let args = parse(&[
        "--server-bot",
        "1=bot.toml",
        "--grace",
        "5",
        "--data",
        "d",
        "--takeover",
        "takeover.toml",
        "--restore-window",
        "9",
        "mode",
        "0.0.0.0:4433",
    ])
    .unwrap();
    assert_eq!(args.data, PathBuf::from("d"));
    assert_eq!(
        (args.times.grace, args.times.restore_window),
        (Duration::from_secs(5), Duration::from_secs(9))
    );
    let bots: Vec<(u32, PathBuf)> = args
        .bots
        .into_iter()
        .map(|bot| (bot.slot, bot.path))
        .collect();
    assert_eq!(bots, [(1, PathBuf::from("bot.toml"))]);
    assert_eq!(args.takeover, Some(PathBuf::from("takeover.toml")));
    assert_eq!(args.mode, PathBuf::from("mode"));
    assert_eq!(args.address, SocketAddr::from(([0, 0, 0, 0], 4433)));
    // The defaults, with no flag but `--data`.
    let plain = parse(&["--data", "d", "mode", "0.0.0.0:4433"]).unwrap();
    assert_eq!(plain.times, SessionTimes::DEFAULT);
    assert!(plain.bots.is_empty() && plain.takeover.is_none());

    for (args, problem) in [
        (
            &["--data", "d", "--grace", "soon", "mode", "0.0.0.0:4433"][..],
            "--grace: soon is not a whole number of seconds: invalid digit found in string",
        ),
        (
            &["--data", "d", "--server-bot", "1", "mode", "0.0.0.0:4433"],
            "1: not <slot>=<orders file>",
        ),
        (
            &[
                "--data",
                "d",
                "--server-bot",
                "x=bot.toml",
                "mode",
                "0.0.0.0:4433",
            ],
            "x: not a slot number: invalid digit found in string",
        ),
        (
            &["--data", "d", "--bot", "1=bot.toml", "mode", "0.0.0.0:4433"],
            "--bot: no such flag",
        ),
        (
            &["--data", "d", "--data", "e", "mode", "0.0.0.0:4433"],
            "--data given twice",
        ),
        (
            &["mode", "0.0.0.0:4433"],
            "--data and its directory are needed",
        ),
        (&["--data"], "--data needs a value"),
        (
            &["--data", "d", "mode"],
            "the mode and the address are needed, and nothing after",
        ),
        (
            &["--data", "d", "mode", "nowhere"],
            "nowhere: not a socket address: invalid socket address syntax",
        ),
    ] {
        let refused = parse(args)
            .err()
            .map(|error| ErrorReport::of(&error).to_string());
        assert_eq!(refused.as_deref(), Some(problem), "{args:?}");
    }
}
