use bevy_ecs::resource::Resource;
use campfire_core::{Book, View};
use campfire_sim::Capability;

use crate::shop::Shop;

/// The mode's shop; none in a mode that has none. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ShopBook(pub Option<Shop>);

impl Book for ShopBook {
    const OWNER: Option<Capability> = Some(Capability::Items);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
