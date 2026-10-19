use campfire_log::internals::round_trip;
use campfire_sim::IdAllocator;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    let failed = ScriptCallFailed {
        tick: Tick::new(9),
        unit: Some(IdAllocator::default().allocate()),
        hook: "on_hit".to_owned(),
        error: "the unit has no param `power`".to_owned(),
    };
    round_trip(&failed);
    // A call for no unit leaves the field out.
    round_trip(&ScriptCallFailed {
        unit: None,
        hook: "on_match_start".to_owned(),
        ..failed
    });
}
