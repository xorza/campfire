use std::ffi::{OsStr, OsString};
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::str::FromStr;

use campfire_net::SlotBotFile;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::XOnlyPublicKey;

use crate::args::error::ArgsError;

pub(crate) mod error;

/// What the command line names: the orders file a bot plays, the player's key file, the data
/// directory, the mode to play, and the server: as its listing gives it, or a local one.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) bot: Option<PathBuf>,
    pub(crate) key: Option<PathBuf>,
    pub(crate) data: Option<PathBuf>,
    pub(crate) mode: PathBuf,
    pub(crate) server: Server,
}

/// The server the client plays on.
#[derive(Debug)]
pub(crate) enum Server {
    /// A server elsewhere, as its listing gives it.
    Remote {
        address: SocketAddr,
        certificate: CertificateHash,
        key: XOnlyPublicKey,
        tick_hz: NonZeroU32,
    },
    /// A local server, on a thread of the client's process, with the bots of `bots`.
    Local { bots: Vec<SlotBotFile> },
}

/// A flag the client takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flag {
    Bot,
    Key,
    Data,
    Local,
    ServerBot,
}

impl Args {
    /// What `args`, the command line after the program, names, its flags in any order before the
    /// mode; an error for each flaw.
    pub(crate) fn parse(args: impl Iterator<Item = OsString>) -> Result<Args, ArgsError> {
        let mut args = args.peekable();
        let (mut bot, mut key, mut data) = (None, None, None);
        let mut local = false;
        let mut bots = Vec::new();
        while let Some(arg) =
            args.next_if(|arg| arg.to_str().is_some_and(|arg| arg.starts_with("--")))
        {
            let flag = Flag::of(&arg).ok_or(ArgsError::UnknownFlag(arg))?;
            let held = match flag {
                Flag::Local => {
                    local = true;
                    continue;
                }
                Flag::ServerBot => {
                    let value = args.next().ok_or(ArgsError::NoValue(flag))?;
                    let bot: SlotBotFile = Args::text(&value)?.parse().map_err(ArgsError::Bot)?;
                    if bot.slot == 0 {
                        return Err(ArgsError::BotInClientSlot);
                    }
                    bots.push(bot);
                    continue;
                }
                Flag::Bot => &mut bot,
                Flag::Key => &mut key,
                Flag::Data => &mut data,
            };
            let value = args.next().ok_or(ArgsError::NoValue(flag))?;
            if held.replace(PathBuf::from(value)).is_some() {
                return Err(ArgsError::Twice(flag));
            }
        }
        let mode = PathBuf::from(args.next().ok_or(ArgsError::NoMode)?);
        let server = if local {
            if data.is_none() {
                return Err(ArgsError::LocalWithoutData);
            }
            if args.next().is_some() {
                return Err(ArgsError::LocalExtra);
            }
            Server::Local { bots }
        } else {
            if !bots.is_empty() {
                return Err(ArgsError::BotWithoutLocal);
            }
            Server::parse(args)?
        };
        Ok(Args {
            bot,
            key,
            data,
            mode,
            server,
        })
    }

    /// The text of `arg`; an error when it is not UTF-8.
    fn text(arg: &OsStr) -> Result<&str, ArgsError> {
        arg.to_str()
            .ok_or_else(|| ArgsError::NotText(arg.to_owned()))
    }
}

impl Server {
    /// The remote server the arguments after the mode name: its address, its certificate's
    /// hash, its key and its tick rate.
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Server, ArgsError> {
        let (Some(address), Some(certificate), Some(key), Some(tick_hz), None) = (
            args.next(),
            args.next(),
            args.next(),
            args.next(),
            args.next(),
        ) else {
            return Err(ArgsError::RemoteArgs);
        };
        let address = Args::text(&address)?;
        let certificate = Args::text(&certificate)?;
        let key = Args::text(&key)?;
        let tick_hz = Args::text(&tick_hz)?;
        Ok(Server::Remote {
            address: address.parse().map_err(|error| ArgsError::Address {
                text: address.to_owned(),
                error,
            })?,
            certificate: certificate
                .parse()
                .map_err(|error| ArgsError::Certificate {
                    text: certificate.to_owned(),
                    error,
                })?,
            key: XOnlyPublicKey::from_str(key).map_err(|error| ArgsError::Key {
                text: key.to_owned(),
                error,
            })?,
            tick_hz: tick_hz.parse().map_err(|error| ArgsError::TickRate {
                text: tick_hz.to_owned(),
                error,
            })?,
        })
    }
}

impl Flag {
    const ALL: [Flag; 5] = [
        Flag::Bot,
        Flag::Key,
        Flag::Data,
        Flag::Local,
        Flag::ServerBot,
    ];

    /// The flag `arg` names; none for text that names no flag.
    fn of(arg: &OsStr) -> Option<Flag> {
        Flag::ALL.into_iter().find(|flag| arg == flag.name())
    }

    /// The flag as the command line writes it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Flag::Bot => "--bot",
            Flag::Key => "--key",
            Flag::Data => "--data",
            Flag::Local => "--local",
            Flag::ServerBot => "--server-bot",
        }
    }
}

#[cfg(test)]
mod tests;
