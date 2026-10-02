use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::mode::mode_book::ModeBook;
use crate::mode::offer::Offer;
use crate::scripts::ctx::Ctx;

/// What each player chose: for each player slot, a row of each choice's values, in the order
/// of the mode's choices, `None` where the player has not chosen.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Choices(pub(crate) Vec<Option<Offer>>);

impl SimResource for Choices {
    const NAME: &'static str = "mode.choices";

    // A row or a value past the mode's choices would be read past their places.
    fn check(&self, world: &World) -> bool {
        let book = world.get_non_send::<Ctx>().and_then(ModeBook::of);
        book.is_none_or(|book| {
            let players = book.teams.players() as usize;
            book.choices.fits(self, players, &book.roster)
        })
    }
}
