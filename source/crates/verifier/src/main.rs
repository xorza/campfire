//! Replays a session log file with the packages under a directory, and prints the state hash
//! after its last tick, in hex.

use std::env;
use std::error::Error;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use campfire_package::PackageStore;
use campfire_protocol::SessionLog;
use campfire_verifier::Replay;

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let (Some(packages), Some(path), None) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: campfire-verifier <packages directory> <session log file>");
        return ExitCode::from(2);
    };
    let path = Path::new(&path);
    match verify(Path::new(&packages), path) {
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

/// The state hash after the last tick of the log file at `path`, replayed with the packages under
/// `packages`.
fn verify(packages: &Path, path: &Path) -> Result<[u8; 32], Box<dyn Error>> {
    let store = PackageStore::scan(packages)?;
    let mut replay = Replay::new(SessionLog::decode(&fs::read(path)?)?, &store)?;
    while replay.run_tick() {}
    Ok(*replay.runner().state_hash().as_bytes())
}
