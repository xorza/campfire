use std::error::Error;
use std::ffi::OsString;

use campfire_common::NotHex;
use campfire_net::{BotArg, SeatBotArgError};
use clap::error::ErrorKind;

use super::*;

/// What clap reads from `args`; its error when it refuses them.
fn line(args: &[&str]) -> Result<CommandLine, clap::Error> {
    CommandLine::try_parse_from(["campfire-client"].iter().chain(args).map(OsString::from))
}

fn read(args: &[&str]) -> Args {
    Args::try_from(line(args).unwrap()).unwrap()
}

/// The error clap gives for `args`, which it refuses.
fn refused(args: &[&str]) -> clap::Error {
    line(args).unwrap_err()
}

/// The error clap gives for `args`, whose value its parser refuses with the error under it.
fn invalid(args: &[&str]) -> clap::Error {
    let error = refused(args);
    assert_eq!(error.kind(), ErrorKind::ValueValidation, "{args:?}");
    error
}

/// A server's key, and its TLS key, as a listing writes them.
const KEY: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

fn tls_key() -> String {
    "03".repeat(32)
}

#[test]
fn the_command_line_names_a_remote_or_a_local_server_or_its_flaw() {
    let (key, tls_key) = (KEY, tls_key());
    let remote = read(&["--key", "k", "mode", "10.0.0.2:4433", &tls_key, key, "30"]);
    assert_eq!((remote.key, remote.mode), (Some("k".into()), "mode".into()));
    assert_eq!(remote.map, None);
    assert!(matches!(
        remote.server,
        PlayedServer::Remote { address, tick_hz, .. }
            if address == SocketAddr::from(([10, 0, 0, 2], 4433)) && tick_hz.get() == 30
    ));

    // The flags in any order, before and after the mode.
    let local = read(&[
        "--server-orders",
        "1=a.toml",
        "--local",
        "mode",
        "--data",
        "d",
        "--server-bot",
        "2=builder",
        "--map",
        "lane",
        "--option",
        "start_money=50000",
        "--pick",
        "0:faction=china",
    ]);
    assert_eq!(local.map, MapName::parse("lane"));
    let PlayedServer::Local {
        bots,
        options,
        seat_picks,
        data,
    } = local.server
    else {
        panic!("a local server");
    };
    assert_eq!(options, ["start_money=50000".parse().unwrap()]);
    assert_eq!(seat_picks, ["0:faction=china".parse().unwrap()]);
    assert_eq!(data, PathBuf::from("d"));
    // The bots of the mode first, then the order scripts.
    let bots: Vec<(u32, BotArg)> = bots
        .into_iter()
        .map(|bot| (bot.seat.get(), bot.bot))
        .collect();
    let builder = BotArg::Mode(DeclaredName::new("builder").unwrap());
    assert_eq!(bots, [(2, builder), (1, BotArg::Orders("a.toml".into()))]);

    for (args, kind) in [
        (&["--local", "mode"][..], ErrorKind::MissingRequiredArgument),
        (
            &["--local", "--data", "d", "mode", "10.0.0.2:4433"],
            ErrorKind::ArgumentConflict,
        ),
        (
            &[
                "--server-orders",
                "1=a.toml",
                "mode",
                "10.0.0.2:4433",
                &tls_key,
                key,
                "30",
            ],
            ErrorKind::MissingRequiredArgument,
        ),
        (&["--fast", "mode"], ErrorKind::UnknownArgument),
        // A pick is the local server's alone.
        (
            &[
                "--option",
                "a=1",
                "mode",
                "10.0.0.2:4433",
                &tls_key,
                key,
                "30",
            ],
            ErrorKind::MissingRequiredArgument,
        ),
        (
            &["--local", "--data", "d", "--map", "3v3", "mode"],
            ErrorKind::ValueValidation,
        ),
        (&["--key"], ErrorKind::InvalidValue),
        (
            &["--key", "a", "--key", "b", "mode"],
            ErrorKind::ArgumentConflict,
        ),
        (
            &["--local", "--data", "d", "--local", "mode"],
            ErrorKind::ArgumentConflict,
        ),
        (&["--key", "a"], ErrorKind::MissingRequiredArgument),
        (
            &["mode", "10.0.0.2:4433"],
            ErrorKind::MissingRequiredArgument,
        ),
        (
            &["mode", "10.0.0.2:4433", &tls_key, key, "30", "x"],
            ErrorKind::UnknownArgument,
        ),
    ] {
        assert_eq!(refused(args).kind(), kind, "{args:?}");
    }
}

#[test]
fn a_value_its_parser_refuses_gives_that_parsers_error_and_the_help_is_no_refusal() {
    let (key, tls_key) = (KEY, tls_key());
    let bot = |flag: &str, text: &str| {
        let error = invalid(&["--local", "--data", "d", flag, text, "mode"]);
        error
            .source()
            .and_then(|error| error.downcast_ref())
            .cloned()
    };
    assert_eq!(
        bot("--server-orders", "a.toml"),
        Some(SeatBotArgError::NotPair("a.toml".to_owned()))
    );
    assert_eq!(
        bot("--server-bot", "1=a.toml"),
        Some(SeatBotArgError::Name("a.toml".to_owned()))
    );
    let wrong_tls_key = invalid(&["mode", "10.0.0.2:4433", "03", key, "30"]);
    assert_eq!(
        wrong_tls_key
            .source()
            .and_then(|error| error.downcast_ref()),
        Some(&NotHex)
    );
    let source = |args: &[&str]| invalid(args).source().map(ToString::to_string);
    let address = |address: &str| {
        invalid(&["mode", address, &tls_key, key, "30"])
            .source()
            .and_then(|error| error.downcast_ref::<ServerAddressError>())
            .cloned()
    };
    assert!(matches!(
        address("nowhere"),
        Some(ServerAddressError::Syntax(_))
    ));
    assert_eq!(
        address("0.0.0.0:4433"),
        Some(ServerAddressError::Unspecified)
    );
    assert_eq!(address("[::]:4433"), Some(ServerAddressError::Unspecified));
    assert_eq!(address("10.0.0.2:0"), Some(ServerAddressError::NoPort));
    assert_eq!(
        source(&["mode", "10.0.0.2:4433", &tls_key, key, "0"]).as_deref(),
        Some("number would be zero for non-zero type")
    );

    // The help and the version are no refusal: clap writes them to standard output, and the
    // client exits with success.
    for (flag, kind) in [
        ("--help", ErrorKind::DisplayHelp),
        ("--version", ErrorKind::DisplayVersion),
    ] {
        let shown = refused(&[flag]);
        assert_eq!(shown.kind(), kind);
        assert!(!shown.use_stderr() && shown.exit_code() == 0, "{flag}");
    }
    assert!(refused(&["--fast", "mode"]).use_stderr());
}

#[test]
fn a_spectator_watches_a_remote_or_a_local_server_and_names_teams_only_as_one() {
    let (key, tls_key) = (KEY, tls_key());
    let remote = ["mode", "10.0.0.2:4433", &tls_key, key, "30"];
    let watch = |flags: &[&str]| line(&[flags, &remote[..]].concat());
    let spectator =
        Args::try_from(watch(&["--spectate", "--watch", "west", "--watch", "east"]).unwrap())
            .unwrap();
    assert_eq!(spectator.role, JoinRole::Watch);
    let names = |names: &[&str]| -> Vec<DeclaredName> {
        names
            .iter()
            .map(|name| DeclaredName::new(name).unwrap())
            .collect()
    };
    assert_eq!(spectator.watch, names(&["west", "east"]));
    // A player, with neither.
    let player = read(&remote);
    assert!(player.role == JoinRole::Play && player.watch.is_empty());
    // A team named with no spectator, a spectator that is a bot, and no team's name.
    for (flags, kind) in [
        (&["--watch", "west"][..], ErrorKind::MissingRequiredArgument),
        (
            &["--spectate", "--bot", "b.toml"],
            ErrorKind::ArgumentConflict,
        ),
        (
            &["--spectate", "--watch", "West"],
            ErrorKind::ValueValidation,
        ),
    ] {
        assert_eq!(watch(flags).unwrap_err().kind(), kind, "{flags:?}");
    }

    // A local server's watcher leaves slot 0 to a bot, which a player keeps for itself.
    let local = [
        "--local",
        "--data",
        "d",
        "--server-orders",
        "0=a.toml",
        "mode",
    ];
    let watcher = read(&[&["--spectate"], &local[..]].concat());
    let PlayedServer::Local { bots, .. } = watcher.server else {
        panic!("a local server");
    };
    assert_eq!(watcher.role, JoinRole::Watch);
    assert_eq!(bots[0].seat, Seat::new(0));
    assert_eq!(
        Args::try_from(line(&local).unwrap()).unwrap_err(),
        ArgsError::ClientSlot
    );
    let team = refused(&[&["--spectate", "--watch", "west"], &local[..]].concat());
    assert_eq!(team.kind(), ErrorKind::ArgumentConflict);
}
