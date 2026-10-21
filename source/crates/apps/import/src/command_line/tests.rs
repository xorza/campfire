use std::ffi::OsString;

use clap::error::ErrorKind;

use super::*;

#[test]
fn the_command_line_names_a_game_an_install_and_a_package() {
    let read = |args: &[&str]| {
        CommandLine::try_parse_from(["campfire-import"].iter().chain(args).map(OsString::from))
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
