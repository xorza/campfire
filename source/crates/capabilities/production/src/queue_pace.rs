use bevy_ecs::system::{Res, SystemParam};
use campfire_actions::ActionBook;
use campfire_common::{PlayerSlot, Ticks};
use campfire_players::PlayerRows;
use campfire_values::ActionId;

use crate::player_power::PlayerPower;
use crate::power_book::PowerBook;
use crate::power_speed::PowerSpeed;

/// How long a queue's entries take: each its own time, or, in a mode whose power speed slows its
/// kind, its time at its player's power (04-capabilities/production.md, A train's end follows the
/// speed).
#[derive(Debug, Clone, Copy)]
pub enum QueuePace<'a> {
    Own,
    Power {
        speed: PowerSpeed,
        power: PlayerPower,
        actions: &'a ActionBook,
    },
}

/// The pace of each player's queues, as a system that changes a queue takes it.
#[derive(SystemParam, Debug)]
pub(crate) struct QueuePaces<'w, 's> {
    book: Res<'w, PowerBook>,
    actions: Res<'w, ActionBook>,
    players: PlayerRows<'w, 's, PlayerPower>,
}

impl<'a> QueuePace<'a> {
    /// The pace of a queue whose player holds `power`, in a mode of `book` and `actions`: its own
    /// for a mode that slows none, or a unit no player of a counted power owns.
    pub(crate) fn of(book: PowerBook, actions: &'a ActionBook, power: Option<PlayerPower>) -> Self {
        match (book.speed(), power) {
            (Some(speed), Some(power)) => Self::Power {
                speed,
                power,
                actions,
            },
            (None, _) | (_, None) => Self::Own,
        }
    }

    /// How long an entry of `action` of its own `time` takes at this pace.
    pub(crate) fn time(self, action: ActionId, time: Ticks) -> Ticks {
        match self {
            Self::Own => time,
            Self::Power {
                speed,
                power,
                actions,
            } => {
                let kind = actions
                    .get(action)
                    .expect("a queue holds actions of the book")
                    .kind;
                if speed.slows(kind) {
                    speed.time_at(time, power)
                } else {
                    time
                }
            }
        }
    }
}

impl QueuePaces<'_, '_> {
    /// The pace of the queues of a unit of the player `owner`.
    pub(crate) fn of(&self, owner: Option<PlayerSlot>) -> QueuePace<'_> {
        let power = owner.and_then(|owner| self.players.of_player_slot(owner).copied());
        QueuePace::of(*self.book, &self.actions, power)
    }
}
