use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::StableId;

/// The strikes that land in the running tick. They apply together after collision, in the order
/// of their source's stable id, so each strike follows from the state before any of them. Not
/// state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Strikes(pub(crate) Vec<Strike>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Strike {
    pub(crate) source: StableId,
    pub(crate) target: StableId,
    pub(crate) amount: Num,
}
