use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::book::Book;
use crate::items::shop::Shop;
use crate::scripts::frame::Frame;
use crate::units::view::View;

/// The mode's shop; none in a mode that has none. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ShopBook(pub(crate) Option<Shop>);

impl Book for ShopBook {
    const OWNER: Option<Capability> = Some(Capability::Items);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}
