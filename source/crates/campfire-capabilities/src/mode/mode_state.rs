use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::mode::mode_book::ModeBook;
use crate::mode::mode_data::ModeData;
use crate::scripts::ctx::Ctx;
use crate::scripts::state_value::StateValue;

/// The mode's script state: a value for each field its data declares, in the order of their
/// names.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeState(pub(crate) Vec<StateValue>);

impl ModeState {
    /// The state `data` declares, each field at its first value, in the order of their names.
    pub(crate) fn initial(data: &ModeData) -> ModeState {
        let fields = data.state.values();
        ModeState(fields.map(|field| field.decl.initial().clone()).collect())
    }

    pub const fn get(&self) -> &[StateValue] {
        self.0.as_slice()
    }
}

impl SimResource for ModeState {
    const NAME: &'static str = "mode.state";

    // A field the mode's state does not declare, or of another type, reaches its script as a
    // value it never wrote.
    fn check(&self, world: &World) -> bool {
        let book = world.get_non_send::<Ctx>().and_then(ModeBook::of);
        book.is_none_or(|book| book.schema.fits_state(&self.0))
    }
}
