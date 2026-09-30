//! Replays a session log file and prints the state hash after its last tick, in hex.

use std::env;
use std::error::Error;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use campfire_protocol::SessionLog;
use campfire_verifier::Replay;

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let (Some(path), None) = (args.next(), args.next()) else {
        eprintln!("usage: campfire-verifier <session log file>");
        return ExitCode::from(2);
    };
    let path = Path::new(&path);
    match verify(path) {
        Ok(hash) => {
            let mut hex = String::with_capacity(2 * hash.len());
            for byte in hash {
                write!(hex, "{byte:02x}").expect("a String takes any write");
            }
            println!("{hex}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}

/// The state hash after the last tick of the log file at `path`.
fn verify(path: &Path) -> Result<[u8; 32], Box<dyn Error>> {
    let log = SessionLog::decode(&fs::read(path)?)?;
    let mut replay = Replay::new(&log)?;
    while let Some(outcome) = replay.next_tick() {
        outcome.expect("a decoded log records its own inputs again");
    }
    Ok(*replay.runner().state_hash().as_bytes())
}
