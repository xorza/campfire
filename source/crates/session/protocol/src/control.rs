use serde::{Deserialize, Serialize};

use crate::delegation::Delegation;
use crate::input_chain::InputChain;

/// Who controls a seat: a player, by their current delegation `D`, with their chain as logged so
/// far; a bot; no one; or no one but the player who left it. One shape for every holder of it:
/// the log keeps a delegation by its index among its entries, a checkpoint's carry the
/// delegation itself, its wire the delegation's JSON, and a reader of the log borrows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Control<D> {
    Player { delegation: D, chain: InputChain },
    Bot,
    Open,
    Reserved,
}

/// Who controls a seat now, as a reader of the log sees it, the player's delegation borrowed from
/// the log.
pub type Controller<'a> = Control<&'a Delegation>;

impl<D> Control<D> {
    /// The same control, its player's delegation given by `f` of theirs.
    pub fn map<E>(self, f: impl FnOnce(D) -> E) -> Control<E> {
        match self {
            Self::Player { delegation, chain } => Control::Player {
                delegation: f(delegation),
                chain,
            },
            Self::Bot => Control::Bot,
            Self::Open => Control::Open,
            Self::Reserved => Control::Reserved,
        }
    }

    /// The same control, its player's delegation given by `f` of theirs; `f`'s error when it
    /// gives one.
    pub fn try_map<E, F>(self, f: impl FnOnce(D) -> Result<E, F>) -> Result<Control<E>, F> {
        Ok(match self {
            Self::Player { delegation, chain } => Control::Player {
                delegation: f(delegation)?,
                chain,
            },
            Self::Bot => Control::Bot,
            Self::Open => Control::Open,
            Self::Reserved => Control::Reserved,
        })
    }

    /// The same control, its player's delegation borrowed.
    pub const fn as_ref(&self) -> Control<&D> {
        match self {
            Self::Player { delegation, chain } => Control::Player {
                delegation,
                chain: *chain,
            },
            Self::Bot => Control::Bot,
            Self::Open => Control::Open,
            Self::Reserved => Control::Reserved,
        }
    }
}
