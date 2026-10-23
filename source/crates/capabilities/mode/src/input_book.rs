use std::collections::BTreeMap;

use bevy_ecs::resource::Resource;
use campfire_common::DeclaredName;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_script::MatchNames;

use crate::mode_data::{InputType, ModeData};

/// The mode's inputs, each by its name, of the type its data declares, as a bot's script sends
/// them. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct InputBook(BTreeMap<DeclaredName, InputType>);

impl InputBook {
    /// The inputs of the mode's data; none for a load of no mode.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder takes the build's signature"
    )]
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let inputs = cx.given::<ModeData>().map(|data| data.inputs.clone());
        Ok(Self(inputs.unwrap_or_default()))
    }

    /// The type of the input `name`; none for a name the mode declares no input of.
    pub fn input_type(&self, name: &DeclaredName) -> Option<InputType> {
        self.0.get(name).copied()
    }
}

impl Book for InputBook {
    const SCRIPTS_ONLY: bool = true;

    fn share(&self, _: &mut MatchNames) {}
}
