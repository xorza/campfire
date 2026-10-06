use campfire_log::ErrorReport;

use super::*;

fn parse(args: &[&str]) -> Result<Args, ArgsError> {
    Args::parse(args.iter().map(OsString::from))
}

#[test]
fn the_command_line_names_a_remote_or_a_local_server_or_its_flaw() {
    let key = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    let certificate = "03".repeat(32);
    let remote = parse(&[
        "--key",
        "k",
        "mode",
        "10.0.0.2:4433",
        &certificate,
        key,
        "30",
    ])
    .unwrap();
    assert_eq!((remote.key, remote.mode), (Some("k".into()), "mode".into()));
    assert!(matches!(
        remote.server,
        Server::Remote { address, tick_hz, .. }
            if address == SocketAddr::from(([10, 0, 0, 2], 4433)) && tick_hz.get() == 30
    ));

    let local = parse(&[
        "--server-bot",
        "1=a.toml",
        "--local",
        "--data",
        "d",
        "--server-bot",
        "2=b.toml",
        "mode",
    ])
    .unwrap();
    assert_eq!(local.data, Some("d".into()));
    let Server::Local { bots } = local.server else {
        panic!("a local server");
    };
    let bots: Vec<(u32, PathBuf)> = bots.into_iter().map(|bot| (bot.slot, bot.path)).collect();
    assert_eq!(bots, [(1, "a.toml".into()), (2, "b.toml".into())]);

    for (args, problem) in [
        (&["--local", "mode"][..], "--local needs --data"),
        (
            &["--local", "--data", "d", "mode", "x"],
            "--local takes the mode alone",
        ),
        (
            &["--server-bot", "1=a.toml", "mode"],
            "--server-bot needs --local",
        ),
        (
            &["--local", "--data", "d", "--server-bot", "0=a.toml", "mode"],
            "slot 0 is the client's",
        ),
        (
            &["--local", "--data", "d", "--server-bot", "a.toml", "mode"],
            "a.toml: not <slot>=<orders file>",
        ),
        (&["--fast", "mode"], "--fast: no such option"),
        (&["--key"], "--key needs a value"),
        (&["--key", "a", "--key", "b", "mode"], "--key given twice"),
        (&["--key", "a"], "the mode is needed after the options"),
        (
            &["mode", "a", "b"],
            "four arguments are needed after the mode",
        ),
        (
            &["mode", "nowhere", &certificate, key, "30"],
            "nowhere: not a socket address: invalid socket address syntax",
        ),
        (
            &["mode", "10.0.0.2:4433", &certificate, key, "0"],
            "0: not a tick rate: number would be zero for non-zero type",
        ),
    ] {
        let refused = parse(args)
            .err()
            .map(|error| ErrorReport::of(&error).to_string());
        assert_eq!(refused.as_deref(), Some(problem), "{args:?}");
    }
}
