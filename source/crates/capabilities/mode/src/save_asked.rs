use bevy_ecs::resource::Resource;
use campfire_common::Tick;
use campfire_declare::Report;

/// The boundary the mode last asked for a save at, by `ctx.save()`: the end of the tick it asked
/// in. Not state: a checkpoint, which the log records, is what a save leaves.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveAsked {
    at: Tick,
}

impl SaveAsked {
    pub const fn new(at: Tick) -> SaveAsked {
        SaveAsked { at }
    }

    /// The boundary, by the tick it comes before.
    pub const fn at(self) -> Tick {
        self.at
    }
}

/// The mode puts it in as it asks for a save.
impl Report for SaveAsked {
    fn installed() -> Option<SaveAsked> {
        None
    }
}
