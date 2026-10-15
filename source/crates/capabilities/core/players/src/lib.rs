//! The players: the match's and the players' entities, their resources, their commands, and
//! their part of a script call.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod capability_action;
mod held_out;
mod on_match;
mod order_units;
mod out_of_play;
mod player_resources;
mod player_rows;
mod players;
mod players_call;
mod players_world;
mod resource_amount;
mod seated;
mod tick_commands;

pub use crate::capability_action::CapabilityAction;
pub use crate::held_out::HeldOut;
pub use crate::on_match::OnMatch;
pub use crate::order_units::OrderUnits;
pub use crate::out_of_play::{Avatars, OutOfPlay};
pub use crate::player_resources::PlayerResources;
pub use crate::player_rows::{PlayerRows, PlayerRowsMut};
pub use crate::players::Players;
pub use crate::players_call::PlayersCall;
pub use crate::players_world::PlayersWorld;
pub use crate::resource_amount::ResourceAmount;
pub use crate::seated::{Seated, Seating, SlotStart};
pub use crate::tick_commands::{TickCommand, TickCommands};
