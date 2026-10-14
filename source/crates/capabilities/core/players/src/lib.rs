//! The players: the match's and the players' entities, their resources, their commands, and
//! their part of a script call.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod players;

pub use crate::players::Players;
pub use crate::players::capability_action::CapabilityAction;
pub use crate::players::held_out::HeldOut;
pub use crate::players::on_match::OnMatch;
pub use crate::players::order_units::OrderUnits;
pub use crate::players::out_of_play::{Avatars, OutOfPlay};
pub use crate::players::player_resources::PlayerResources;
pub use crate::players::player_rows::{PlayerRows, PlayerRowsMut};
pub use crate::players::players_call::PlayersCall;
pub use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};
pub use crate::players::resource_amount::ResourceAmount;
pub use crate::players::tick_commands::{TickCommand, TickCommands};
