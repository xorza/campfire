use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Imports a game's install into a package of its own, the same bytes on every machine.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct Args {
    /// The game the install holds
    pub(crate) game: Game,
    /// The install's directory, as the game runs from it
    pub(crate) install: PathBuf,
    /// The package's directory, which the import makes, and refuses when it is there
    pub(crate) out: PathBuf,
}

/// The games the importer reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Game {
    /// Command & Conquer: Generals – Zero Hour, with base Generals beside it
    ZeroHour,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use clap::error::ErrorKind;

    use super::*;

    #[test]
    fn the_command_line_names_a_game_an_install_and_a_package() {
        let read = |args: &[&str]| {
            Args::try_parse_from(["campfire-import"].iter().chain(args).map(OsString::from))
        };
        let args = read(&["zero-hour", "/games/zh", "out"]).unwrap();
        assert_eq!(args.game, Game::ZeroHour);
        assert_eq!(args.install, PathBuf::from("/games/zh"));
        assert_eq!(args.out, PathBuf::from("out"));
        assert_eq!(
            read(&["red-alert", "i", "o"]).unwrap_err().kind(),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            read(&["zero-hour", "i"]).unwrap_err().kind(),
            ErrorKind::MissingRequiredArgument
        );
    }
}
