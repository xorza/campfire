use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Play a LAN match and check it, logging into `dir`.
    Play { dir: PathBuf },
    /// Verify the session log of a match played in `dir`, perhaps on another machine, with this
    /// machine's verifier, and compare the server's final hash.
    Verify { dir: PathBuf },
}

impl Mode {
    /// The mode `args` name: `[<run directory>]`, or `verify <run directory>`; `None` for any
    /// other command line. A match logs into `campfire-lan-check` in the temporary directory when
    /// the command line names no directory.
    pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Option<Mode> {
        let mode = match (args.next(), args.next()) {
            (None, _) => Mode::Play {
                dir: env::temp_dir().join("campfire-lan-check"),
            },
            (Some(verify), Some(dir)) if verify == "verify" => Mode::Verify {
                dir: PathBuf::from(dir),
            },
            (Some(dir), None) if dir != "verify" => Mode::Play {
                dir: PathBuf::from(dir),
            },
            _ => return None,
        };
        args.next().is_none().then_some(mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_names_a_mode_and_its_directory() {
        let parse = |args: &[&str]| Mode::parse(args.iter().map(OsString::from));
        let dir = PathBuf::from("run");
        assert_eq!(
            parse(&[]),
            Some(Mode::Play {
                dir: env::temp_dir().join("campfire-lan-check")
            })
        );
        assert_eq!(parse(&["run"]), Some(Mode::Play { dir: dir.clone() }));
        assert_eq!(parse(&["verify", "run"]), Some(Mode::Verify { dir }));
        for flawed in [
            &["verify"][..],
            &["run", "more"],
            &["verify", "run", "more"],
        ] {
            assert_eq!(parse(flawed), None, "{flawed:?}");
        }
    }
}
