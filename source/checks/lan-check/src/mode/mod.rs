use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Play a LAN match and check it, logging into a new directory below `root`.
    Play { root: PathBuf },
    /// Verify the session log of a match played in `dir`, perhaps on another machine, with this
    /// machine's verifier, and compare the server's final hash.
    Verify { dir: PathBuf },
}

impl Mode {
    /// The mode `args` name: `[<run root>]`, or `verify <run directory>`; `None` for any other
    /// command line. The run root is `campfire-lan-check` in the temporary directory when the
    /// command line names none.
    pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Option<Mode> {
        let mode = match (args.next(), args.next()) {
            (None, _) => Mode::Play {
                root: env::temp_dir().join("campfire-lan-check"),
            },
            (Some(verify), Some(dir)) if verify == "verify" => Mode::Verify {
                dir: PathBuf::from(dir),
            },
            (Some(root), None) if root != "verify" => Mode::Play {
                root: PathBuf::from(root),
            },
            _ => return None,
        };
        args.next().is_none().then_some(mode)
    }
}

#[cfg(test)]
mod tests;
