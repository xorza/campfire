//! The players: the match's and the players' entities, their resources, their commands, and
//! their part of a script call.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod capability_action;
mod held_out;
mod match_entity;
mod on_match;
mod order_units;
mod out_of_play;
mod player;
mod player_resources;
mod player_rows;
mod players;
mod players_call;
mod players_world;
mod resource_amount;
mod resource_book;
mod resources_data;
mod seated;
mod tick_commands;

pub use crate::capability_action::CapabilityAction;
pub use crate::held_out::HeldOut;
pub use crate::match_entity::MatchEntity;
pub use crate::on_match::OnMatch;
pub use crate::order_units::OrderUnits;
pub use crate::out_of_play::OutOfPlay;
pub use crate::player::Player;
pub use crate::player_resources::PlayerResources;
pub use crate::player_rows::{PlayerRows, PlayerRowsMut};
pub use crate::players::Players;
pub use crate::players_call::PlayersCall;
pub use crate::players_world::PlayersWorld;
pub use crate::resource_amount::ResourceAmount;
pub use crate::resource_book::ResourceBook;
pub use crate::resources_data::ResourcesData;
pub use crate::seated::{SeatStart, Seating};
pub use crate::tick_commands::{TickCommand, TickCommands};
