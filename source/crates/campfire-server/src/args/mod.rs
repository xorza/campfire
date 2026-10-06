use std::ffi::{OsStr, OsString};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use campfire_net::{OrderScript, OrderScriptReadError, ServerBots, SessionTimes, SlotBotFile};

use crate::args::error::ArgsError;

pub(crate) mod error;

/// What the command line names: the data directory, how long after its journal's last write a
/// session a stop ended restores, the server's bots, the mode to open, and the address to listen
/// on.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) data: PathBuf,
    pub(crate) times: SessionTimes,
    pub(crate) bots: Vec<SlotBotFile>,
    pub(crate) takeover: Option<PathBuf>,
    pub(crate) mode: PathBuf,
    pub(crate) address: SocketAddr,
}

/// A flag the server takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flag {
    Data,
    Grace,
    RestoreWindow,
    ServerBot,
    Takeover,
}

impl Args {
    /// What `args`, the command line after the program, names, its flags in any order before the
    /// mode and the address; an error for each flaw.
    pub(crate) fn parse(args: impl Iterator<Item = OsString>) -> Result<Args, ArgsError> {
        let mut args = args.peekable();
        let (mut data, mut grace, mut restore_window, mut takeover) = (None, None, None, None);
        let mut bots = Vec::new();
        while let Some(arg) =
            args.next_if(|arg| arg.to_str().is_some_and(|arg| arg.starts_with("--")))
        {
            let flag = Flag::of(&arg).ok_or(ArgsError::UnknownFlag(arg))?;
            let value = args.next().ok_or(ArgsError::NoValue(flag))?;
            let twice = match flag {
                Flag::Data => data.replace(PathBuf::from(value)).is_some(),
                Flag::Takeover => takeover.replace(PathBuf::from(value)).is_some(),
                Flag::Grace => grace.replace(Args::seconds(flag, &value)?).is_some(),
                Flag::RestoreWindow => restore_window
                    .replace(Args::seconds(flag, &value)?)
                    .is_some(),
                Flag::ServerBot => {
                    bots.push(Args::text(&value)?.parse().map_err(ArgsError::Bot)?);
                    false
                }
            };
            if twice {
                return Err(ArgsError::Twice(flag));
            }
        }
        let data = data.ok_or(ArgsError::NoData)?;
        let (Some(mode), Some(address), None) = (args.next(), args.next(), args.next()) else {
            return Err(ArgsError::Positionals);
        };
        let text = Args::text(&address)?;
        let address = text.parse().map_err(|error| ArgsError::Address {
            text: text.to_owned(),
            error,
        })?;
        let defaults = SessionTimes::DEFAULT;
        Ok(Args {
            data,
            times: SessionTimes {
                grace: grace.unwrap_or(defaults.grace),
                restore_window: restore_window.unwrap_or(defaults.restore_window),
            },
            bots,
            takeover,
            mode: PathBuf::from(mode),
            address,
        })
    }

    /// The server's bots, their scripts read from their files; an error naming a file that does
    /// not read.
    pub(crate) fn server_bots(&self) -> Result<ServerBots, OrderScriptReadError> {
        let slots = self
            .bots
            .iter()
            .map(SlotBotFile::read)
            .collect::<Result<_, _>>()?;
        let takeover = self
            .takeover
            .as_deref()
            .map(OrderScript::read)
            .transpose()?;
        Ok(ServerBots { slots, takeover })
    }

    /// The text of `arg`; an error when it is not UTF-8.
    fn text(arg: &OsStr) -> Result<&str, ArgsError> {
        arg.to_str()
            .ok_or_else(|| ArgsError::NotText(arg.to_owned()))
    }

    /// The whole seconds `value` of `flag` names.
    fn seconds(flag: Flag, value: &OsStr) -> Result<Duration, ArgsError> {
        let text = Args::text(value)?;
        let seconds = text.parse().map_err(|error| ArgsError::Seconds {
            flag,
            text: text.to_owned(),
            error,
        })?;
        Ok(Duration::from_secs(seconds))
    }
}

impl Flag {
    const ALL: [Flag; 5] = [
        Flag::Data,
        Flag::Grace,
        Flag::RestoreWindow,
        Flag::ServerBot,
        Flag::Takeover,
    ];

    /// The flag `arg` names; none for text that names no flag.
    fn of(arg: &OsStr) -> Option<Flag> {
        Flag::ALL.into_iter().find(|flag| arg == flag.name())
    }

    /// The flag as the command line writes it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Flag::Data => "--data",
            Flag::Grace => "--grace",
            Flag::RestoreWindow => "--restore-window",
            Flag::ServerBot => "--server-bot",
            Flag::Takeover => "--takeover",
        }
    }
}

#[cfg(test)]
mod tests;
