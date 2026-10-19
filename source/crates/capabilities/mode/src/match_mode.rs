use std::ops::Deref;
use std::sync::Arc;

use bevy_ecs::resource::Resource;

use crate::mode_spec::ModeSpec;

/// The mode of the match, once it installs, as its systems and checks read it. A script call
/// reads the same book through the mode's part of its frame, which holds another handle of it.
#[derive(Resource, Debug, Clone)]
pub struct MatchMode(pub(crate) Arc<ModeSpec>);

impl Deref for MatchMode {
    type Target = ModeSpec;

    fn deref(&self) -> &ModeSpec {
        &self.0
    }
}
