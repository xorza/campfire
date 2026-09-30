use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

use campfire_math::PlayerSlot;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::XOnlyPublicKey;
use campfire_sim::{StateHash, Tick};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// The events of the processes' logs that the check reads, by their messages, with the fields it
/// reads; every other message is `Other`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "message")]
pub(crate) enum KnownEvent {
    /// The server listens, and players join with its certificate hash and key.
    #[serde(rename = "listening; players join with the command in `join`")]
    Listening {
        #[serde(deserialize_with = "text")]
        certificate: CertificateHash,
        #[serde(deserialize_with = "text")]
        server_key: XOnlyPublicKey,
    },
    /// A client's match started, and the server gave it `slot`.
    #[serde(rename = "the match started")]
    MatchStarted { slot: PlayerSlot },
    /// A client sent `orders` orders, each stamped `stamp`.
    #[serde(rename = "sent orders")]
    SentOrders { stamp: Tick, orders: usize },
    /// The server logged an input of `slot`, which takes effect in `tick`.
    #[serde(rename = "logged an input")]
    LoggedInput {
        slot: PlayerSlot,
        stamp: Tick,
        tick: Tick,
    },
    /// The server wrote the session log to `file`, in its working directory, after a match that
    /// ended in the state of `hash`.
    #[serde(
        rename = "every player left; wrote the session log, which `campfire-verifier \
                      <packages directory> <file>` replays to the same hash"
    )]
    WroteLog {
        file: PathBuf,
        #[serde(deserialize_with = "text")]
        hash: StateHash,
    },
    /// The verifier replayed a log to the state of `hash`.
    #[serde(rename = "the log verifies; its final state hash")]
    Verified {
        #[serde(deserialize_with = "text")]
        hash: StateHash,
    },
    #[serde(other)]
    Other,
}

/// A value its text spells, as `Display` writes it.
fn text<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr<Err: Display>,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(D::Error::custom)
}
