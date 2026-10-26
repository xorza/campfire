use bevy_ecs::resource::Resource;
use campfire_common::Tick;

/// The sim tick the newest state the client received from its server stands before, as the
/// client's own sim tick stands after the ticks it ran: the newest message's tick through the
/// match clock (`MatchClock::next_after`); none while the client neither plays nor watches, or
/// holds no message.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReceivedTick(Option<Tick>);

impl ReceivedTick {
    pub const fn new(tick: Option<Tick>) -> Self {
        Self(tick)
    }

    pub const fn get(self) -> Option<Tick> {
        self.0
    }
}
